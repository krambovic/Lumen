"""Keep the UI and owned network cores schedulable during CPU-heavy games.

Only the normal Windows priority class is raised.  Existing user-selected
priority classes are left alone, and helper/check processes stay at normal
priority.  This is deliberately ABOVE_NORMAL, never HIGH or REALTIME.
"""
from __future__ import annotations

import ctypes
import os


NORMAL_PRIORITY_CLASS = 0x00000020
ABOVE_NORMAL_PRIORITY_CLASS = 0x00008000
CORE_RUN_PRIORITY_FLAG = ABOVE_NORMAL_PRIORITY_CLASS if os.name == "nt" else 0


def prioritize_lumen_ui() -> bool:
    """Raise only a normal-priority Lumen process; failures are non-fatal."""
    if os.name != "nt":
        return False
    try:
        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel32.GetCurrentProcess.restype = ctypes.c_void_p
        kernel32.GetPriorityClass.argtypes = [ctypes.c_void_p]
        kernel32.GetPriorityClass.restype = ctypes.c_uint32
        kernel32.SetPriorityClass.argtypes = [ctypes.c_void_p, ctypes.c_uint32]
        kernel32.SetPriorityClass.restype = ctypes.c_int
        process = kernel32.GetCurrentProcess()
        if kernel32.GetPriorityClass(process) != NORMAL_PRIORITY_CLASS:
            return False
        return bool(kernel32.SetPriorityClass(process, ABOVE_NORMAL_PRIORITY_CLASS))
    except (AttributeError, OSError, ValueError):
        return False
