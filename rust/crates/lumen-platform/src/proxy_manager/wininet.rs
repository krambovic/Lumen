use std::ffi::c_void;
use std::io;
use std::os::windows::io::{FromRawHandle, OwnedHandle};

use anyhow::Result;
use windows::Win32::Foundation::{ERROR_INVALID_PARAMETER, FILETIME, HANDLE};
use windows::Win32::Networking::WinInet::{
    INTERNET_OPTION_PER_CONNECTION_OPTION, INTERNET_OPTION_REFRESH,
    INTERNET_OPTION_SETTINGS_CHANGED, INTERNET_PER_CONN, INTERNET_PER_CONN_AUTOCONFIG_URL,
    INTERNET_PER_CONN_FLAGS, INTERNET_PER_CONN_OPTION_LISTW, INTERNET_PER_CONN_OPTIONW,
    INTERNET_PER_CONN_OPTIONW_0, INTERNET_PER_CONN_PROXY_BYPASS, INTERNET_PER_CONN_PROXY_SERVER,
    InternetQueryOptionW, InternetSetOptionW, PROXY_TYPE_PROXY,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::core::PWSTR;
use winreg::enums::{KEY_READ, KEY_SET_VALUE};

use crate::inet_settings::open_internet_settings;

use super::{NativeError, ProxySettings, Snapshot, SystemProxy};

pub(super) struct WinInet;

fn read_or_default<T: winreg::types::FromRegValue + Default>(
    key: &winreg::RegKey,
    name: &str,
) -> io::Result<T> {
    match key.get_value(name) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(T::default()),
        result => result,
    }
}

impl SystemProxy for WinInet {
    fn read(&self) -> Result<ProxySettings> {
        let key = open_internet_settings(KEY_READ)?;
        Ok(ProxySettings {
            proxy_enable: read_or_default(&key, "ProxyEnable")?,
            proxy_server: read_or_default(&key, "ProxyServer")?,
            proxy_override: read_or_default(&key, "ProxyOverride")?,
            auto_config_url: read_or_default(&key, "AutoConfigURL")?,
        })
    }

    fn write(&self, settings: &ProxySettings) -> Result<()> {
        let key = open_internet_settings(KEY_SET_VALUE)?;
        key.set_value("ProxyEnable", &settings.proxy_enable)?;
        key.set_value("ProxyServer", &settings.proxy_server)?;
        key.set_value("ProxyOverride", &settings.proxy_override)?;
        key.set_value("AutoConfigURL", &settings.auto_config_url)?;
        Ok(())
    }

    fn flags(&self) -> Result<u32> {
        let mut options = [flags_option(0)];
        let mut list = lan_options(&mut options);
        let mut length = list.dwSize;
        // The query only returns a DWORD. No WinINet-allocated strings are requested.
        unsafe {
            InternetQueryOptionW(
                None,
                INTERNET_OPTION_PER_CONNECTION_OPTION,
                Some((&mut list as *mut INTERNET_PER_CONN_OPTION_LISTW).cast()),
                &mut length,
            )?;
            Ok(options[0].Value.dwValue)
        }
    }

    fn apply(&self, snapshot: &Snapshot) -> std::result::Result<(), NativeError> {
        let settings = &snapshot.settings;
        let mut server = utf16_z(&settings.proxy_server);
        let mut bypass = utf16_z(&settings.proxy_override);
        let mut pac = utf16_z(&settings.auto_config_url);
        let mut options = vec![flags_option(snapshot.flags)];
        if settings.proxy_enable != 0 || snapshot.flags & PROXY_TYPE_PROXY != 0 {
            options.push(string_option(INTERNET_PER_CONN_PROXY_SERVER, &mut server));
            if !settings.proxy_override.is_empty() {
                options.push(string_option(INTERNET_PER_CONN_PROXY_BYPASS, &mut bypass));
            }
        }
        // Passing an empty PAC option can fail on Windows builds which
        // already accepted the registry value. Only send a real URL here.
        if !settings.auto_config_url.is_empty() {
            options.push(string_option(INTERNET_PER_CONN_AUTOCONFIG_URL, &mut pac));
        }
        let mut list = lan_options(&mut options);
        // The option array and all UTF-16 buffers remain alive through the call.
        unsafe {
            InternetSetOptionW(
                None,
                INTERNET_OPTION_PER_CONNECTION_OPTION,
                Some((&mut list as *mut INTERNET_PER_CONN_OPTION_LISTW).cast::<c_void>()),
                list.dwSize,
            )
        }
        .map_err(|error| {
            let code = error.code().0;
            let code = if (code as u32) >> 16 == 0x8007 {
                code & 0xffff
            } else {
                code
            };
            NativeError {
                code,
                option: list.dwOptionError,
            }
        })
    }

    fn refresh(&self) {
        unsafe {
            let _ = InternetSetOptionW(None, INTERNET_OPTION_SETTINGS_CHANGED, None, 0);
            let _ = InternetSetOptionW(None, INTERNET_OPTION_REFRESH, None, 0);
        }
    }

    fn process_created(&self, pid: u32) -> Result<Option<u64>> {
        if pid == std::process::id() {
            return Ok(Some(created_at(unsafe { GetCurrentProcess() })?));
        }
        let handle = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
            Ok(handle) => handle,
            Err(error) if error.code() == ERROR_INVALID_PARAMETER.to_hresult() => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let _owned = unsafe { OwnedHandle::from_raw_handle(handle.0) };
        Ok(Some(created_at(handle)?))
    }
}

fn created_at(handle: HANDLE) -> windows::core::Result<u64> {
    let [mut created, mut exited, mut kernel, mut user] = [FILETIME::default(); 4];
    unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user)? };
    Ok((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
}

fn flags_option(flags: u32) -> INTERNET_PER_CONN_OPTIONW {
    INTERNET_PER_CONN_OPTIONW {
        dwOption: INTERNET_PER_CONN_FLAGS,
        Value: INTERNET_PER_CONN_OPTIONW_0 { dwValue: flags },
    }
}

fn string_option(option: INTERNET_PER_CONN, text: &mut [u16]) -> INTERNET_PER_CONN_OPTIONW {
    INTERNET_PER_CONN_OPTIONW {
        dwOption: option,
        Value: INTERNET_PER_CONN_OPTIONW_0 {
            pszValue: PWSTR(text.as_mut_ptr()),
        },
    }
}

fn lan_options(options: &mut [INTERNET_PER_CONN_OPTIONW]) -> INTERNET_PER_CONN_OPTION_LISTW {
    INTERNET_PER_CONN_OPTION_LISTW {
        dwSize: size_of::<INTERNET_PER_CONN_OPTION_LISTW>() as u32,
        pszConnection: PWSTR::null(),
        dwOptionCount: options.len() as u32,
        dwOptionError: 0,
        pOptions: options.as_mut_ptr(),
    }
}

fn utf16_z(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}
