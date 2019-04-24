use std::fs::File;
use std::io::{Read, Result, Write};
use std::mem;
use std::os::windows::io::{FromRawHandle, RawHandle};
use std::ptr::{null, null_mut};
use widestring::U16CString;
use winapi::shared::basetsd::{PSIZE_T, SIZE_T};
use winapi::shared::minwindef::BYTE;
use winapi::shared::ntdef::LPWSTR;
use winapi::um::consoleapi::{ClosePseudoConsole, CreatePseudoConsole, ResizePseudoConsole};
use winapi::um::namedpipeapi::CreatePipe;
use winapi::um::processthreadsapi::{
    CreateProcessW, InitializeProcThreadAttributeList, UpdateProcThreadAttribute,
    PROCESS_INFORMATION, STARTUPINFOW,
};
use winapi::um::winbase::{EXTENDED_STARTUPINFO_PRESENT, STARTUPINFOEXW};
use winapi::um::wincontypes::{COORD, HPCON};
use winapi::um::winnt::HANDLE;

pub struct Pty {
    handle: HPCON,
}

impl Drop for Pty {
    fn drop(&mut self) {
        unsafe {
            ClosePseudoConsole(self.handle);
        }
    }
}

impl Pty {
    pub fn resize(&self, cols: u32, rows: u32) {
        let y = cols as i16;
        let x = rows as i16;
        unsafe {
            ResizePseudoConsole(self.handle, COORD { X: x, Y: y });
        }
    }

    pub fn spawn(cmdline: &str) -> Result<(Self, impl Read, impl Write)> {
        let cwd = "C:\\";

        let mut pipe_in: HANDLE = null_mut();
        let mut pipe_out: HANDLE = null_mut();
        let mut pipe_pty_in: HANDLE = null_mut();
        let mut pipe_pty_out: HANDLE = null_mut();
        let mut handle = null_mut();

        unsafe {
            CreatePipe(&mut pipe_pty_in, &mut pipe_in, null_mut(), 0);
            CreatePipe(&mut pipe_out, &mut pipe_pty_out, null_mut(), 0);
            CreatePseudoConsole(
                COORD { X: 80, Y: 20 },
                pipe_pty_in,
                pipe_pty_out,
                0,
                &mut handle,
            );

            let mut si_ex: STARTUPINFOEXW = { mem::zeroed() };
            si_ex.StartupInfo.cb = mem::size_of::<STARTUPINFOEXW>() as u32;

            let mut size: SIZE_T = 0;
            InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut size as PSIZE_T);

            let mut attr_list: Box<[BYTE]> = vec![0; size].into_boxed_slice();
            si_ex.lpAttributeList = attr_list.as_mut_ptr() as _;

            InitializeProcThreadAttributeList(si_ex.lpAttributeList, 1, 0, &mut size as PSIZE_T);
            UpdateProcThreadAttribute(
                si_ex.lpAttributeList,
                0,
                22 | 0x0002_0000, // PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE
                handle,
                mem::size_of::<HPCON>(),
                null_mut(),
                null_mut(),
            );

            let mut proc_info: PROCESS_INFORMATION = { mem::zeroed() };

            CreateProcessW(
                null(),
                U16CString::from_str(cmdline).unwrap().as_ptr() as LPWSTR,
                null_mut(),
                null_mut(),
                false as i32,
                EXTENDED_STARTUPINFO_PRESENT,
                null_mut(),
                U16CString::from_str(cwd).unwrap().as_ptr(),
                &mut si_ex.StartupInfo as *mut STARTUPINFOW,
                &mut proc_info as *mut PROCESS_INFORMATION,
            );

            let file_in = File::from_raw_handle(pipe_in as RawHandle);
            let file_out = File::from_raw_handle(pipe_out as RawHandle);

            Ok((Pty { handle }, file_out, file_in))
        }
    }
}
