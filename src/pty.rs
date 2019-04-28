use std::fs::File;
use std::io::{Read, Result, Write};
use std::mem;
use std::os::windows::io::{FromRawHandle, RawHandle};
use std::ptr::{null, null_mut};
use std::sync::mpsc::{channel, Receiver, SendError, Sender, TryRecvError};
use std::thread;

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
    send: Sender<u8>,
    recv: Receiver<u8>,
}

pub struct PtyConfig<'a> {
    pub shell: &'a str,
    pub cols: u32,
    pub rows: u32,
    pub cwd: &'a str,
}

impl<'a> Default for PtyConfig<'a> {
    fn default() -> Self {
        Self {
            shell: "powershell",
            cols: 80,
            rows: 24,
            cwd: "C:\\",
        }
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        unsafe {
            ClosePseudoConsole(self.handle);
        }
    }
}

impl Pty {
    pub fn send(&self, byte: u8) -> std::result::Result<(), SendError<u8>> {
        self.send.send(byte)
    }

    pub fn try_receive(&self) -> std::result::Result<u8, TryRecvError> {
        self.recv.try_recv()
    }

    pub fn resize(&self, cols: u32, rows: u32) {
        let y = cols as i16;
        let x = rows as i16;
        unsafe {
            ResizePseudoConsole(self.handle, COORD { X: x, Y: y });
        }
    }

    pub fn spawn(config: &PtyConfig) -> Result<Self> {
        let mut pipe_in: HANDLE = null_mut();
        let mut pipe_out: HANDLE = null_mut();
        let mut pipe_pty_in: HANDLE = null_mut();
        let mut pipe_pty_out: HANDLE = null_mut();
        let mut handle = null_mut();

        // TODO(seikichi): Reduce unsafe block and close resources correctly.
        unsafe {
            CreatePipe(&mut pipe_pty_in, &mut pipe_in, null_mut(), 0);
            CreatePipe(&mut pipe_out, &mut pipe_pty_out, null_mut(), 0);
            CreatePseudoConsole(
                COORD {
                    X: config.cols as i16,
                    Y: config.rows as i16,
                },
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
                U16CString::from_str(config.shell).unwrap().as_ptr() as LPWSTR,
                null_mut(),
                null_mut(),
                false as i32,
                EXTENDED_STARTUPINFO_PRESENT,
                null_mut(),
                U16CString::from_str(config.cwd).unwrap().as_ptr(),
                &mut si_ex.StartupInfo as *mut STARTUPINFOW,
                &mut proc_info as *mut PROCESS_INFORMATION,
            );

            let mut file_in = File::from_raw_handle(pipe_in as RawHandle);
            let mut file_out = File::from_raw_handle(pipe_out as RawHandle);

            let (send_in, recv_in) = channel();
            let (send_out, recv_out) = channel();

            thread::spawn(move || loop {
                let mut buffer = [0; 32];
                let n = file_out.read(&mut buffer).unwrap();
                for b in &buffer[..n] {
                    send_out.send(*b).unwrap();
                }
            });

            thread::spawn(move || loop {
                let b = recv_in.recv().unwrap();
                file_in.write(&[b]).unwrap();
            });

            Ok(Pty {
                handle,
                recv: recv_out,
                send: send_in,
            })
        }
    }
}
