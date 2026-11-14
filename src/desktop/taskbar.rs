//! Thumbnail toolbar, owned by the UI thread. Playback updates arrive through a channel.
use crate::player::{Player, TransportState};
use anyhow::{Context, Result, ensure};
use gpui::{App, BorrowAppContext, Global};
use std::{cell::Cell, ptr};
use windows::Win32::{
    Foundation::HWND as ComHwnd,
    System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
    UI::{
        Shell::{
            ITaskbarList3, THB_FLAGS, THB_ICON, THB_TOOLTIP, THBF_DISABLED, THBF_ENABLED,
            THBN_CLICKED, THUMBBUTTON, TaskbarList,
        },
        WindowsAndMessaging::HICON as ComIcon,
    },
};
use windows_sys::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::*,
        },
    },
    core::w,
};

const SUBCLASS_ID: usize = 0x53485244;
const PREVIOUS: u32 = 2001;
const TOGGLE: u32 = 2002;
const NEXT: u32 = 2003;

struct State {
    hwnd: HWND,
    api: ITaskbarList3,
    icons: [HICON; 4],
    player: Player,
    transport: Cell<TransportState>,
    added: Cell<bool>,
    attached: Cell<bool>,
    button_created: u32,
}
struct Taskbar(Box<State>);
impl Global for Taskbar {}

pub(super) fn refresh_language(cx: &mut App) {
    if cx.has_global::<Taskbar>() {
        cx.global::<Taskbar>().0.update_buttons();
    }
}

pub(super) fn install(hwnd: HWND, player: Player, cx: &mut App) -> Result<()> {
    let receiver = player.subscribe_transport();
    let initial = receiver
        .try_recv()
        .context("Не удалось прочитать состояние плеера")?;
    let taskbar = unsafe { Taskbar::new(hwnd, player, initial)? };
    // The registered message handles a not-yet-ready taskbar and Explorer restarts.
    taskbar.0.add_buttons();
    cx.set_global(taskbar);
    cx.on_app_quit(|cx| {
        cx.update_global::<Taskbar, _>(|taskbar, _| taskbar.detach());
        async {}
    })
    .detach();
    cx.spawn(async move |cx| {
        while let Ok(transport) = receiver.recv().await {
            if cx
                .update(|cx| {
                    cx.update_global::<Taskbar, _>(|taskbar, _| {
                        taskbar.0.transport.set(transport);
                        taskbar.0.update_buttons();
                    })
                })
                .is_err()
            {
                break;
            }
        }
    })
    .detach();
    Ok(())
}

impl Taskbar {
    unsafe fn new(hwnd: HWND, player: Player, transport: TransportState) -> Result<Self> {
        unsafe {
            // GPUI initializes OLE/COM on this UI thread and owns its apartment lifetime.
            let api: ITaskbarList3 = CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER)?;
            api.HrInit()?;
            let instance = GetModuleHandleW(ptr::null());
            let mut icons = [ptr::null_mut(); 4];
            for (index, icon) in icons.iter_mut().enumerate() {
                *icon = LoadImageW(
                    instance,
                    (102 + index) as *const u16,
                    IMAGE_ICON,
                    32,
                    32,
                    LR_SHARED,
                ) as HICON;
                ensure!(!icon.is_null(), "Не удалось загрузить кнопки панели задач");
            }
            let state = Box::new(State {
                hwnd,
                api,
                icons,
                player,
                transport: Cell::new(transport),
                added: Cell::new(false),
                attached: Cell::new(false),
                button_created: RegisterWindowMessageW(w!("TaskbarButtonCreated")),
            });
            ensure!(
                state.button_created != 0,
                "Не удалось зарегистрировать панель задач"
            );
            ensure!(
                SetWindowSubclass(
                    hwnd,
                    Some(window_proc),
                    SUBCLASS_ID,
                    (&*state as *const State) as usize
                ) != 0,
                "Не удалось подключить кнопки панели задач"
            );
            state.attached.set(true);
            Ok(Self(state))
        }
    }
    fn detach(&self) {
        if self.0.attached.replace(false) {
            unsafe {
                RemoveWindowSubclass(self.0.hwnd, Some(window_proc), SUBCLASS_ID);
            }
        }
    }
}
impl Drop for Taskbar {
    fn drop(&mut self) {
        self.detach();
    }
}

impl State {
    fn buttons(&self) -> [THUMBBUTTON; 3] {
        let transport = self.transport.get();
        let paused = transport.paused || !transport.has_track;
        let middle = if paused {
            (1, "Продолжить")
        } else {
            (2, "Пауза")
        };
        [
            (PREVIOUS, 0, "Предыдущий трек"),
            (TOGGLE, middle.0, middle.1),
            (NEXT, 3, "Следующий трек"),
        ]
        .map(|(id, icon, tip)| {
            let mut button = THUMBBUTTON {
                dwMask: THB_ICON | THB_TOOLTIP | THB_FLAGS,
                iId: id,
                hIcon: ComIcon(self.icons[icon]),
                dwFlags: if transport.has_track {
                    THBF_ENABLED
                } else {
                    THBF_DISABLED
                },
                ..Default::default()
            };
            for (target, source) in button
                .szTip
                .iter_mut()
                .zip(shard::i18n::tr(tip).encode_utf16())
            {
                *target = source;
            }
            button
        })
    }
    fn add_buttons(&self) {
        let added = unsafe {
            self.api
                .ThumbBarAddButtons(ComHwnd(self.hwnd), &self.buttons())
        }
        .is_ok();
        // An initial add may precede TaskbarButtonCreated. Duplicate adds are rejected;
        // update the existing toolbar in that case to keep state synchronization active.
        self.added.set(
            added
                || unsafe {
                    self.api
                        .ThumbBarUpdateButtons(ComHwnd(self.hwnd), &self.buttons())
                }
                .is_ok(),
        );
    }
    fn update_buttons(&self) {
        if self.attached.get() && self.added.get() {
            let _ = unsafe {
                self.api
                    .ThumbBarUpdateButtons(ComHwnd(self.hwnd), &self.buttons())
            };
        }
    }
}

fn clicked_button(message: u32, wparam: WPARAM) -> Option<u32> {
    if message != WM_COMMAND || ((wparam >> 16) & 0xffff) as u32 != THBN_CLICKED {
        return None;
    }
    match (wparam & 0xffff) as u32 {
        id @ (PREVIOUS | TOGGLE | NEXT) => Some(id),
        _ => None,
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    reference: usize,
) -> LRESULT {
    unsafe {
        let state = &*(reference as *const State);
        if message == state.button_created {
            state.add_buttons();
        } else if let Some(id) = clicked_button(message, wparam) {
            if state.transport.get().has_track {
                match id {
                    PREVIOUS => state.player.previous(),
                    TOGGLE => state.player.toggle(),
                    NEXT => state.player.next(),
                    _ => {}
                }
            }
            return 0;
        } else if message == WM_NCDESTROY {
            state.attached.set(false);
            RemoveWindowSubclass(hwnd, Some(window_proc), SUBCLASS_ID);
        }
        DefSubclassProc(hwnd, message, wparam, lparam)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    #[test]
    fn only_thumbnail_notifications_dispatch_playback_commands() {
        for id in [PREVIOUS, TOGGLE, NEXT] {
            assert_eq!(
                clicked_button(WM_COMMAND, ((THBN_CLICKED as usize) << 16) | id as usize),
                Some(id)
            );
            assert_eq!(clicked_button(WM_COMMAND, id as usize), None);
        }
        assert_eq!(
            clicked_button(WM_COMMAND, ((THBN_CLICKED as usize) << 16) | 999),
            None
        );
        assert_eq!(
            clicked_button(WM_APP, ((THBN_CLICKED as usize) << 16) | NEXT as usize),
            None
        );
    }

    #[test]
    fn native_thumbnail_clicks_control_the_audio_worker() -> Result<()> {
        use crate::player::PlaybackTrack;
        use windows::Win32::System::Com::{
            COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize,
        };
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        }
        struct Apartment;
        impl Drop for Apartment {
            fn drop(&mut self) {
                unsafe {
                    CoUninitialize();
                }
            }
        }
        let _apartment = Apartment;
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                w!("STATIC"),
                w!("Shard transport test"),
                WS_OVERLAPPEDWINDOW,
                0,
                0,
                320,
                200,
                ptr::null_mut(),
                ptr::null_mut(),
                GetModuleHandleW(ptr::null()),
                ptr::null(),
            )
        };
        ensure!(!hwnd.is_null(), "Test window was not created");
        struct TestWindow(HWND);
        impl Drop for TestWindow {
            fn drop(&mut self) {
                unsafe {
                    DestroyWindow(self.0);
                }
            }
        }
        let _window = TestWindow(hwnd);
        let root = tempfile::tempdir()?;
        let wav = root.path().join("silent.wav");
        let data_len = 320_000u32; // Twenty seconds of silence, no user files or audible output.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(data_len + 36).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        for value in [1u16, 1] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        for value in [8000u32, 16000] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        for value in [2u16, 16] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        bytes.resize(44 + data_len as usize, 0);
        std::fs::write(&wav, bytes)?;
        let player = Player::new(0., false, None);
        let transport = player.subscribe_transport();
        assert!(!transport.try_recv()?.has_track);
        let taskbar = unsafe {
            Taskbar::new(
                hwnd,
                player.clone(),
                TransportState {
                    has_track: true,
                    paused: false,
                },
            )?
        };
        taskbar.0.add_buttons();
        ensure!(
            taskbar.0.added.get(),
            "Windows rejected the thumbnail toolbar"
        );
        let wait = |id, paused| -> Result<()> {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let state = player.state();
                ensure!(state.error.is_none(), "Audio error: {:?}", state.error);
                if state.track_id == Some(id) && state.paused == paused {
                    return Ok(());
                }
                ensure!(
                    Instant::now() < deadline,
                    "Thumbnail command was not applied"
                );
                std::thread::sleep(Duration::from_millis(20));
            }
        };
        player.start(
            (1..=3)
                .map(|id| PlaybackTrack {
                    id,
                    path: wav.clone(),
                })
                .collect(),
            2,
        );
        wait(2, false)?;
        assert!(transport.try_recv()?.has_track);
        let click = |id| unsafe {
            SendMessageW(
                hwnd,
                WM_COMMAND,
                ((THBN_CLICKED as usize) << 16) | id as usize,
                0,
            );
        };
        click(TOGGLE);
        wait(2, true)?;
        assert!(transport.try_recv()?.paused);
        click(TOGGLE);
        wait(2, false)?;
        assert!(!transport.try_recv()?.paused);
        click(NEXT);
        wait(3, false)?;
        click(PREVIOUS);
        wait(2, false)?;
        player.stop();
        player.flush_and_wait();
        drop(taskbar); // Detaches the native subclass before destroying its window.
        Ok(())
    }
}
