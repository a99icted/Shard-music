//! Native notification-area icon and taskbar controls on the existing Windows message loop.
#[path = "desktop/taskbar.rs"]
mod taskbar;
use crate::player::Player;
use anyhow::{Context, Result, bail};
use gpui::{App, BorrowAppContext, Global, Window};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{mem::size_of, ptr};
use windows_sys::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::{Shell::*, WindowsAndMessaging::*},
    },
    core::w,
};

const CALLBACK: u32 = WM_APP + 41;
#[derive(Clone, Copy)]
enum Event {
    Show,
    Hide,
    Quit,
}

struct State {
    events: async_channel::Sender<Event>,
    icon: NOTIFYICONDATAW,
    taskbar_created: u32,
}
struct Tray {
    hwnd: HWND,
    state: *mut State,
}
impl Global for Tray {}

pub(crate) fn refresh_language(cx: &mut App) {
    taskbar::refresh_language(cx);
}

pub(crate) fn hwnd(window: &Window) -> Result<HWND> {
    let handle = HasWindowHandle::window_handle(window)
        .map_err(|error| anyhow::anyhow!("Не удалось получить окно Windows: {error}"))?;
    match handle.as_raw() {
        RawWindowHandle::Win32(handle) => Ok(handle.hwnd.get() as HWND),
        _ => bail!("Не удалось получить окно Windows"),
    }
}

pub fn install(window: &mut Window, cx: &mut App, player: Player) -> Result<()> {
    let main = hwnd(window)? as usize;
    // Taskbar controls are independent of whether the notification area is available.
    if let Err(error) = taskbar::install(main as HWND, player, cx) {
        eprintln!("Shard: управление на панели задач недоступно: {error:#}");
    }
    let (sender, receiver) = async_channel::unbounded();
    let tray = unsafe { Tray::new(sender)? };
    unsafe {
        let icon = tray
            .state
            .as_ref()
            .context("Не удалось создать значок")?
            .icon
            .hIcon;
        SendMessageW(main as HWND, WM_SETICON, ICON_BIG as usize, icon as isize);
        SendMessageW(main as HWND, WM_SETICON, ICON_SMALL as usize, icon as isize);
    }
    cx.set_global(tray);
    window.on_window_should_close(cx, move |_, _| {
        unsafe {
            ShowWindow(main as HWND, SW_HIDE);
        }
        false
    });
    cx.on_app_quit(|cx| {
        cx.update_global::<Tray, _>(|tray, _| tray.remove());
        async {}
    })
    .detach();
    let handle = Window::window_handle(window);
    cx.spawn(async move |cx| {
        while let Ok(event) = receiver.recv().await {
            let _ = cx.update(|cx| match event {
                Event::Show => {
                    unsafe {
                        let hwnd = main as HWND;
                        ShowWindow(
                            hwnd,
                            if IsIconic(hwnd) != 0 {
                                SW_RESTORE
                            } else {
                                SW_SHOW
                            },
                        );
                        SetForegroundWindow(hwnd);
                    }
                    let _ = handle.update(cx, |_, window, cx| {
                        window.activate_window();
                        cx.refresh_windows();
                    });
                }
                Event::Hide => unsafe {
                    ShowWindow(main as HWND, SW_HIDE);
                },
                Event::Quit => cx.quit(),
            });
        }
    })
    .detach();
    Ok(())
}

impl Tray {
    unsafe fn new(events: async_channel::Sender<Event>) -> Result<Self> {
        unsafe {
            let instance = GetModuleHandleW(ptr::null());
            let class = w!("ShardTrayWindow");
            let window_class = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance,
                lpszClassName: class,
                ..Default::default()
            };
            RegisterClassW(&window_class);
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                class,
                w!("Shard"),
                0,
                0,
                0,
                0,
                0,
                ptr::null_mut(),
                ptr::null_mut(),
                instance,
                ptr::null(),
            );
            if hwnd.is_null() {
                return Err(std::io::Error::last_os_error())
                    .context("Не удалось создать трей Shard");
            }
            let icon = LoadImageW(
                instance,
                101usize as *const u16,
                IMAGE_ICON,
                32,
                32,
                LR_SHARED,
            ) as HICON;
            if icon.is_null() {
                DestroyWindow(hwnd);
                return Err(std::io::Error::last_os_error())
                    .context("Не удалось загрузить значок Shard");
            }
            let mut data = NOTIFYICONDATAW {
                cbSize: size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: 1,
                uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
                uCallbackMessage: CALLBACK,
                hIcon: icon,
                ..Default::default()
            };
            let tip: Vec<_> = "Shard".encode_utf16().collect();
            data.szTip[..tip.len()].copy_from_slice(&tip);
            let state = Box::into_raw(Box::new(State {
                events,
                icon: data,
                taskbar_created: RegisterWindowMessageW(w!("TaskbarCreated")),
            }));
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize);
            let tray = Self { hwnd, state };
            if Shell_NotifyIconW(NIM_ADD, &data) == 0 {
                bail!("Windows не удалось добавить значок Shard в трей");
            }
            Ok(tray)
        }
    }
    fn remove(&mut self) {
        if !self.hwnd.is_null() {
            unsafe {
                Shell_NotifyIconW(NIM_DELETE, &(*self.state).icon);
                SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
                DestroyWindow(self.hwnd);
                drop(Box::from_raw(self.state));
            }
            self.hwnd = ptr::null_mut();
        }
    }
}
impl Drop for Tray {
    fn drop(&mut self) {
        self.remove();
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
        if let Some(state) = state.as_ref() {
            if state.taskbar_created != 0 && message == state.taskbar_created {
                Shell_NotifyIconW(NIM_ADD, &state.icon);
                return 0;
            }
            if message == CALLBACK {
                match lparam as u32 {
                    WM_LBUTTONUP | WM_LBUTTONDBLCLK => {
                        let _ = state.events.try_send(Event::Show);
                    }
                    WM_RBUTTONUP | WM_CONTEXTMENU => {
                        let menu = CreatePopupMenu();
                        if !menu.is_null() {
                            let label = |ru, en| if shard::i18n::is_english() { en } else { ru };
                            AppendMenuW(
                                menu,
                                MF_STRING,
                                1,
                                label(w!("Открыть Shard"), w!("Open Shard")),
                            );
                            AppendMenuW(
                                menu,
                                MF_STRING,
                                2,
                                label(w!("Скрыть в трей"), w!("Hide to tray")),
                            );
                            AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
                            AppendMenuW(menu, MF_STRING, 3, label(w!("Выйти"), w!("Quit")));
                            let mut point = POINT::default();
                            GetCursorPos(&mut point);
                            SetForegroundWindow(hwnd);
                            let selected = TrackPopupMenu(
                                menu,
                                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                                point.x,
                                point.y,
                                0,
                                hwnd,
                                ptr::null(),
                            );
                            DestroyMenu(menu);
                            let event = match selected {
                                1 => Some(Event::Show),
                                2 => Some(Event::Hide),
                                3 => Some(Event::Quit),
                                _ => None,
                            };
                            if let Some(event) = event {
                                let _ = state.events.try_send(event);
                            }
                            PostMessageW(hwnd, WM_NULL, 0, 0);
                        }
                    }
                    _ => {}
                }
                return 0;
            }
        }
        DefWindowProcW(hwnd, message, wparam, lparam)
    }
}
