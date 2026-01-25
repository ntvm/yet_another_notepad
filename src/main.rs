#![windows_subsystem = "windows"]

use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::Graphics::Gdi::*,
    Win32::System::LibraryLoader::GetModuleHandleW,
    Win32::UI::WindowsAndMessaging::*,
    Win32::UI::Input::KeyboardAndMouse::SetFocus, // Добавлено
    Win32::UI::Controls::*,
};

// Исправлено: используем null_mut() для указателя внутри HWND
static mut HWND_EDIT: HWND = HWND(std::ptr::null_mut());

fn main() -> Result<()> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let window_class = w!("MyMinimalNotepad");

        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance.into(),
            lpszClassName: window_class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            // Исправлено: приведение индекса цвета к указателю для HBRUSH
            hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as *mut core::ffi::c_void),
            ..Default::default()
        };

        RegisterClassW(&wc);

        let _hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            window_class,
            w!("Minimal Notepad"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT, CW_USEDEFAULT, 800, 600,
            None, None, instance, None,
        )?; // Добавлен ? для обработки Result

        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }

        Ok(())
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            let instance = GetModuleHandleW(None).unwrap();
            
            // Создаем системный контрол EDIT
            let edit_res = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("EDIT"),
                None,
                WS_CHILD | WS_VISIBLE | WS_VSCROLL | WS_HSCROLL | 
                WINDOW_STYLE(ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32 | ES_WANTRETURN as u32),
                0, 0, 0, 0,
                hwnd,
                // Исправлено: приведение ID к указателю для HMENU
                HMENU(101 as *mut core::ffi::c_void),
                instance,
                None,
            );

            match edit_res {
                Ok(h) => {
                    HWND_EDIT = h;
                    // Устанавливаем шрифт Consolas
                    let h_font = CreateFontW(
                        19, 0, 0, 0, 400, 0, 0, 0, 
                        DEFAULT_CHARSET.0 as u32, 
                        OUT_DEFAULT_PRECIS.0 as u32, 
                        CLIP_DEFAULT_PRECIS.0 as u32, 
                        CLEARTYPE_QUALITY.0 as u32, 
                        VARIABLE_PITCH.0 as u32, 
                        w!("Consolas")
                    );
                    SendMessageW(HWND_EDIT, WM_SETFONT, WPARAM(h_font.0 as usize), LPARAM(1));
                }
                Err(_) => return LRESULT(-1),
            }
            LRESULT(0)
        }
        WM_SIZE => {
            let width = (lparam.0 & 0xFFFF) as i32;
            let height = ((lparam.0 >> 16) & 0xFFFF) as i32;
            if !HWND_EDIT.0.is_null() {
                let _ = MoveWindow(HWND_EDIT, 0, 0, width, height, true);
            }
            LRESULT(0)
        }
        WM_SETFOCUS => {
            if !HWND_EDIT.0.is_null() {
                SetFocus(HWND_EDIT);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}