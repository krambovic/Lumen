//! Windows single-instance ownership and local activation transport.

use std::ffi::c_void;
use std::io;
use std::mem::size_of;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::{
    ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{Instant, sleep, timeout};
use windows::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, ERROR_NO_DATA, ERROR_PIPE_BUSY, ERROR_PIPE_NOT_CONNECTED, GetLastError,
    HANDLE, HLOCAL, HWND, LocalFree, SetLastError, WIN32_ERROR,
};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{
    GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
    TokenUser,
};
use windows::Win32::Storage::FileSystem::SECURITY_IDENTIFICATION;
use windows::Win32::System::Pipes::GetNamedPipeServerProcessId;
use windows::Win32::System::Threading::{
    CreateMutexW, GetCurrentProcess, OpenMutexW, OpenProcess, OpenProcessToken,
    PROCESS_QUERY_LIMITED_INFORMATION, SYNCHRONIZATION_SYNCHRONIZE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, BringWindowToTop, MB_ICONWARNING, MessageBoxW, SW_RESTORE,
    SetForegroundWindow, ShowWindow,
};
use windows::core::{PCWSTR, PWSTR, w};

const POLL_INTERVAL: Duration = Duration::from_millis(100);
const ORDINARY_WAIT: Duration = Duration::from_secs(5);
const RELAUNCH_WAIT: Duration = Duration::from_secs(20);

#[derive(Clone, Copy)]
struct InstanceNames<'a> {
    mutex: &'a str,
    pipe: &'a str,
    legacy_mutex: &'a str,
    legacy_pipe: &'a str,
}

const NAMES: InstanceNames<'_> = InstanceNames {
    mutex: r"Local\Lumen.SingleInstance",
    pipe: r"\\.\pipe\Lumen.SingleInstance",
    legacy_mutex: r"Local\LumenKVN.SingleInstance",
    legacy_pipe: r"\\.\pipe\LumenKVN.SingleInstance",
};

pub enum InstanceLaunch {
    Primary(PrimaryInstance),
    Forwarded,
    Unavailable,
}

pub struct PrimaryInstance {
    messages: mpsc::Receiver<Vec<u8>>,
    task: Option<JoinHandle<()>>,
    // Keep ownership even if the listener fails while the application is open.
    _mutex: OwnedHandle,
}

impl PrimaryInstance {
    pub async fn recv(&mut self) -> Option<Vec<u8>> {
        self.messages.recv().await
    }

    pub async fn close(mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
            let _ = task.await;
        }
    }
}

impl Drop for PrimaryInstance {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

/// Acquire the Windows mutex or deliver a launch message to the current or legacy owner.
pub async fn acquire_or_forward(
    message: &[u8],
    relaunching: bool,
    foreground: bool,
    max_message_bytes: usize,
) -> InstanceLaunch {
    let launch =
        acquire_with_names(NAMES, message, relaunching, foreground, max_message_bytes).await;
    if matches!(launch, InstanceLaunch::Primary(_)) {
        let _ = std::fs::remove_file(std::env::temp_dir().join("LumenKVN.SingleInstance.lock"));
    }
    launch
}

async fn acquire_with_names(
    names: InstanceNames<'_>,
    message: &[u8],
    relaunching: bool,
    foreground: bool,
    max_message_bytes: usize,
) -> InstanceLaunch {
    let Ok(user_sid) = process_user_sid(unsafe { GetCurrentProcess() }) else {
        return InstanceLaunch::Unavailable;
    };
    let message = &message[..message.len().min(max_message_bytes)];
    let legacy_running = mutex_exists(names.legacy_mutex);
    if forward_message(names.legacy_pipe, &user_sid, message, foreground).await {
        return InstanceLaunch::Forwarded;
    }
    if legacy_running {
        let deadline = Instant::now() + ORDINARY_WAIT;
        while Instant::now() < deadline {
            if forward_message(names.legacy_pipe, &user_sid, message, foreground).await {
                return InstanceLaunch::Forwarded;
            }
            sleep(POLL_INTERVAL).await;
        }
        return InstanceLaunch::Unavailable;
    }

    let deadline = Instant::now()
        + if relaunching {
            RELAUNCH_WAIT
        } else {
            ORDINARY_WAIT
        };
    loop {
        if let Ok(Some(mutex)) = try_create_mutex(names.mutex)
            && let Ok(server) = create_pipe(names.pipe, &user_sid, true)
        {
            return InstanceLaunch::Primary(serve(
                server,
                names.pipe,
                &user_sid,
                mutex,
                max_message_bytes,
            ));
        }
        if forward_message(names.pipe, &user_sid, message, foreground).await {
            return InstanceLaunch::Forwarded;
        }
        if Instant::now() >= deadline {
            return InstanceLaunch::Unavailable;
        }
        sleep(POLL_INTERVAL).await;
    }
}

fn try_create_mutex(name: &str) -> windows::core::Result<Option<OwnedHandle>> {
    let name = wide(name);
    // The open handle keeps the named object alive. This uses existence rather
    // than a thread-owned lock, so the listener can hold it on any Tokio thread.
    let (handle, exists) = unsafe {
        SetLastError(WIN32_ERROR(0));
        let handle = CreateMutexW(None, false, PCWSTR(name.as_ptr()))?;
        (handle, GetLastError() == ERROR_ALREADY_EXISTS)
    };
    let handle = unsafe { OwnedHandle::from_raw_handle(handle.0) };
    Ok((!exists).then_some(handle))
}

fn mutex_exists(name: &str) -> bool {
    let name = wide(name);
    unsafe { OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, PCWSTR(name.as_ptr())) }
        .map(|handle| unsafe { OwnedHandle::from_raw_handle(handle.0) })
        .is_ok()
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

struct LocalAllocation(HLOCAL);

impl Drop for LocalAllocation {
    fn drop(&mut self) {
        unsafe {
            let _ = LocalFree(Some(self.0));
        }
    }
}

fn process_user_sid(process: HANDLE) -> windows::core::Result<String> {
    let mut token = HANDLE::default();
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token)? };
    let _token_owner = unsafe { OwnedHandle::from_raw_handle(token.0) };
    let mut size = 0;
    unsafe {
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut size);
    }
    // TOKEN_USER contains a pointer into this buffer and requires pointer alignment.
    let mut buffer = vec![0_usize; (size as usize).div_ceil(size_of::<usize>()).max(1)];
    unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            Some(buffer.as_mut_ptr().cast()),
            size,
            &mut size,
        )?;
        let user = &*buffer.as_ptr().cast::<TOKEN_USER>();
        let mut sid = PWSTR::null();
        ConvertSidToStringSidW(user.User.Sid, &mut sid)?;
        let _sid_owner = LocalAllocation(HLOCAL(sid.0.cast()));
        sid.to_string().map_err(windows::core::Error::from)
    }
}

fn create_pipe(name: &str, user_sid: &str, first: bool) -> io::Result<NamedPipeServer> {
    let sddl = wide(&format!("D:P(A;;GA;;;{user_sid})"));
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(sddl.as_ptr()),
            SDDL_REVISION_1,
            &mut descriptor,
            None,
        )
    }
    .map_err(io::Error::other)?;
    let _descriptor_owner = LocalAllocation(HLOCAL(descriptor.0));
    let mut attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: false.into(),
    };
    // CreateNamedPipe copies the descriptor before returning.
    unsafe {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .create_with_security_attributes_raw(
                name,
                (&mut attributes as *mut SECURITY_ATTRIBUTES).cast::<c_void>(),
            )
    }
}

fn serve(
    mut server: NamedPipeServer,
    pipe: &str,
    user_sid: &str,
    mutex: OwnedHandle,
    max_message_bytes: usize,
) -> PrimaryInstance {
    let (messages, receiver) = mpsc::channel(32);
    let pipe = pipe.to_owned();
    let user_sid = user_sid.to_owned();
    let task = tokio::spawn(async move {
        loop {
            let connected = server.connect().await;
            if connected
                .as_ref()
                .is_err_and(|error| !is_closed_pipe(error))
            {
                break;
            }
            // A fresh listener avoids carrying Tokio's buffered read or EOF
            // from the previous client into the next connection.
            let Ok(next) = create_pipe(&pipe, &user_sid, false) else {
                break;
            };
            let mut client = std::mem::replace(&mut server, next);
            let message = if connected.is_ok() {
                timeout(
                    Duration::from_secs(1),
                    read_message(&mut client, max_message_bytes),
                )
                .await
                .ok()
                .and_then(Result::ok)
            } else {
                None
            };
            let _ = client.disconnect();
            drop(client);
            if let Some(message) = message
                && messages.send(message).await.is_err()
            {
                break;
            }
        }
    });
    PrimaryInstance {
        messages: receiver,
        task: Some(task),
        _mutex: mutex,
    }
}

fn is_closed_pipe(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::BrokenPipe
        || error.raw_os_error().is_some_and(|code| {
            code == ERROR_NO_DATA.0 as i32 || code == ERROR_PIPE_NOT_CONNECTED.0 as i32
        })
}

async fn read_message(server: &mut (impl AsyncRead + Unpin), limit: usize) -> io::Result<Vec<u8>> {
    let mut message = Vec::new();
    let mut buffer = [0; 4096];
    while message.len() < limit {
        let remaining = (limit - message.len()).min(buffer.len());
        let n = match server.read(&mut buffer[..remaining]).await {
            Ok(n) => n,
            Err(error) if is_closed_pipe(&error) => 0,
            Err(error) => return Err(error),
        };
        if n == 0 {
            break;
        }
        if let Some(end) = buffer[..n].iter().position(|&byte| byte == b'\n') {
            message.extend_from_slice(&buffer[..=end]);
            break;
        }
        message.extend_from_slice(&buffer[..n]);
    }
    Ok(message)
}

async fn forward_message(pipe: &str, user_sid: &str, message: &[u8], foreground: bool) -> bool {
    let client = timeout(Duration::from_millis(250), async {
        loop {
            match ClientOptions::new()
                .security_qos_flags(SECURITY_IDENTIFICATION.0)
                .open(pipe)
            {
                Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY.0 as i32) => {
                    sleep(Duration::from_millis(25)).await;
                }
                result => return result,
            }
        }
    })
    .await;
    let Ok(Ok(mut client)) = client else {
        return false;
    };
    let Ok(Some(pid)) = verified_peer(&client, user_sid) else {
        return false;
    };
    if foreground {
        // Transfer the launcher's foreground permission only to the verified primary.
        unsafe {
            let _ = AllowSetForegroundWindow(pid);
        }
    }
    if !matches!(
        timeout(Duration::from_millis(500), client.write_all(message)).await,
        Ok(Ok(()))
    ) {
        return false;
    }
    // Qt and Rust close the pipe after receiving the message.
    let _ = timeout(Duration::from_millis(750), client.read(&mut [0_u8; 1])).await;
    true
}

fn verified_peer(client: &NamedPipeClient, user_sid: &str) -> windows::core::Result<Option<u32>> {
    let mut pid = 0;
    unsafe {
        GetNamedPipeServerProcessId(HANDLE(client.as_raw_handle()), &mut pid)?;
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)? };
    let _process_owner = unsafe { OwnedHandle::from_raw_handle(process.0) };
    Ok((process_user_sid(process)? == user_sid).then_some(pid))
}

pub fn show_unavailable_warning() {
    unsafe {
        MessageBoxW(
            None,
            w!(
                "Другой экземпляр Lumen уже запущен, но не отвечает.\n\nЗакройте Lumen через значок в области уведомлений или Диспетчер задач, затем запустите эту копию снова."
            ),
            w!("Lumen уже запущен"),
            MB_ICONWARNING,
        );
    }
}

pub fn activate_window(hwnd: HWND) {
    if hwnd.is_invalid() {
        return;
    }
    unsafe {
        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_names() -> [String; 4] {
        let id = uuid::Uuid::new_v4();
        [
            format!(r"Local\Lumen.Test.{id}"),
            format!(r"\\.\pipe\Lumen.Test.{id}"),
            format!(r"Local\Lumen.Legacy.Test.{id}"),
            format!(r"\\.\pipe\Lumen.Legacy.Test.{id}"),
        ]
    }

    fn names(values: &[String; 4]) -> InstanceNames<'_> {
        let [mutex, pipe, legacy_mutex, legacy_pipe] = values;
        InstanceNames {
            mutex,
            pipe,
            legacy_mutex,
            legacy_pipe,
        }
    }

    #[tokio::test]
    async fn forwards_to_primary_and_reacquires_after_it_closes() {
        let values = unique_names();
        let names = names(&values);
        let message = b"{\"command\":\"tray\"}\n";
        let InstanceLaunch::Primary(mut primary) =
            acquire_with_names(names, message, false, false, 1024).await
        else {
            panic!("first launch should own the mutex and pipe");
        };
        for _ in 0..3 {
            assert!(matches!(
                acquire_with_names(names, message, false, false, 1024).await,
                InstanceLaunch::Forwarded
            ));
            assert_eq!(
                timeout(Duration::from_secs(2), primary.recv())
                    .await
                    .unwrap()
                    .as_deref(),
                Some(message.as_slice())
            );
        }
        primary.close().await;
        let InstanceLaunch::Primary(next) =
            acquire_with_names(names, message, false, false, 1024).await
        else {
            panic!("new launch should acquire after the owner exits");
        };
        next.close().await;
    }

    #[tokio::test]
    async fn forwards_to_legacy_primary_before_claiming_the_current_mutex() -> anyhow::Result<()> {
        let values = unique_names();
        let names = names(&values);
        let user = process_user_sid(unsafe { GetCurrentProcess() })?;
        let legacy_mutex = try_create_mutex(names.legacy_mutex)?.unwrap();
        let server = create_pipe(names.legacy_pipe, &user, true)?;
        let mut primary = serve(server, names.legacy_pipe, &user, legacy_mutex, 1024);
        let message = b"{\"command\":\"deeplink\",\"url\":\"lumen://add\"}\n";
        assert!(matches!(
            acquire_with_names(names, message, false, false, 1024).await,
            InstanceLaunch::Forwarded
        ));
        assert_eq!(
            timeout(Duration::from_secs(2), primary.recv())
                .await?
                .as_deref(),
            Some(message.as_slice())
        );
        assert!(!mutex_exists(names.mutex));
        primary.close().await;
        Ok(())
    }

    #[tokio::test]
    async fn acquires_when_a_starting_primary_disappears_before_listening() -> anyhow::Result<()> {
        let values = unique_names();
        let names = names(&values);
        let lock = try_create_mutex(names.mutex)?.unwrap();
        let startup = acquire_with_names(names, b"activate\n", false, false, 1024);
        let release = async {
            sleep(Duration::from_millis(200)).await;
            drop(lock);
        };
        let (outcome, ()) = tokio::join!(startup, release);
        let InstanceLaunch::Primary(primary) = outcome else {
            panic!("a disappeared primary should release its mutex");
        };
        primary.close().await;
        Ok(())
    }

    #[tokio::test]
    async fn reads_at_most_one_bounded_frame() -> io::Result<()> {
        assert_eq!(
            read_message(&mut b"activate\nignored".as_slice(), 1024).await?,
            b"activate\n"
        );
        assert_eq!(
            read_message(&mut b"0123456789".as_slice(), 5).await?,
            b"01234"
        );
        assert_eq!(
            read_message(&mut b"activate".as_slice(), 1024).await?,
            b"activate"
        );
        Ok(())
    }

    #[tokio::test]
    async fn abandoned_client_does_not_stop_the_listener() -> anyhow::Result<()> {
        let values = unique_names();
        let names = names(&values);
        let user = process_user_sid(unsafe { GetCurrentProcess() })?;
        let mutex = try_create_mutex(names.mutex)?.unwrap();
        let server = create_pipe(names.pipe, &user, true)?;
        let mut primary = serve(server, names.pipe, &user, mutex, 1024);
        drop(ClientOptions::new().open(names.pipe)?);
        let message = b"activate\n";
        assert!(forward_message(names.pipe, &user, message, false).await);
        let delivered = timeout(Duration::from_secs(2), async {
            while let Some(payload) = primary.recv().await {
                if !payload.is_empty() {
                    return payload;
                }
            }
            Vec::new()
        })
        .await?;
        assert_eq!(delivered, message);
        primary.close().await;
        Ok(())
    }
}
