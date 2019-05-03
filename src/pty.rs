use std::error::Error;
use std::fs::File;
use std::io::{Read, Write};
use std::mem;
use std::os::windows::io::{FromRawHandle, RawHandle};
use std::ptr::{null, null_mut};
use std::sync::mpsc::{channel, RecvError, Sender};
use std::thread;

use widestring::U16CString;
use winapi::shared::basetsd::{PSIZE_T, SIZE_T};
use winapi::shared::minwindef::BYTE;
use winapi::shared::ntdef::LPWSTR;
use winapi::um::consoleapi::{ClosePseudoConsole, CreatePseudoConsole, ResizePseudoConsole};
use winapi::um::handleapi::CloseHandle;
use winapi::um::namedpipeapi::CreatePipe;
use winapi::um::processthreadsapi::{
    CreateProcessW, InitializeProcThreadAttributeList, UpdateProcThreadAttribute,
    PROCESS_INFORMATION, STARTUPINFOW,
};
use winapi::um::winbase::{EXTENDED_STARTUPINFO_PRESENT, STARTUPINFOEXW};
use winapi::um::wincontypes::{COORD, HPCON};
use winapi::um::winnt::HANDLE;

use vte::Parser;
pub use vte::Perform as Handler;

#[derive(Clone, Debug)]
pub struct Pty {
    tx: Sender<Action>,
}

#[derive(Clone, Debug, PartialEq)]
enum Action {
    Resize(i16, i16),
    Write(u8),
}

struct PtyInner {
    handle: HPCON,
    pipe_in: HANDLE,
    pipe_out: HANDLE,
}

// TODO: is it correct?
unsafe impl Send for PtyInner {}

impl Drop for PtyInner {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.pipe_in);
            CloseHandle(self.pipe_out);
            ClosePseudoConsole(self.handle);
        }
    }
}

pub struct Config<'a> {
    pub shell: &'a str,
    pub cols: u32,
    pub rows: u32,
    pub cwd: &'a str,
}

impl<'a> Default for Config<'a> {
    fn default() -> Self {
        Self {
            shell: "powershell",
            cols: 80,
            rows: 24,
            cwd: "C:\\",
        }
    }
}

impl Pty {
    pub fn resize(&self, cols: u32, rows: u32) -> Result<(), Box<Error>> {
        self.tx.send(Action::Resize(cols as i16, rows as i16))?;
        Ok(())
    }

    pub fn write(&self, b: u8) -> Result<(), Box<Error>> {
        self.tx.send(Action::Write(b))?;
        Ok(())
    }

    pub fn spawn<H: Handler + Send + 'static>(
        config: &Config,
        mut handler: H,
    ) -> Result<Self, Box<Error>> {
        let (tx, rx) = channel::<Action>();

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

            // TODO(seikichi): modify drop to delete STARTUPINFOEXW and lpAttributeList
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

            let mut parser = Parser::new();

            thread::spawn(move || loop {
                let mut buffer = [0u8; 1024];
                match file_out.read(&mut buffer) {
                    Ok(n) if n > 0 => {
                        for b in &buffer[..n] {
                            parser.advance(&mut handler, *b);
                        }
                    }
                    _ => {
                        break;
                    }
                }
            });

            let inner = PtyInner {
                handle,
                pipe_in: pipe_pty_in,
                pipe_out: pipe_pty_out,
            };

            thread::spawn(move || loop {
                match rx.recv() {
                    Ok(Action::Write(b)) => {
                        file_in.write(&[b]).unwrap();
                    }
                    Ok(Action::Resize(x, y)) => {
                        ResizePseudoConsole(inner.handle, COORD { X: x, Y: y });
                    }
                    Err(RecvError) => {
                        break;
                    }
                }
            });

            Ok(Pty { tx })
        }
    }
}
