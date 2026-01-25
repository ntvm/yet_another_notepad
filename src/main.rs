#![windows_subsystem = "windows"] // Чтобы не открывалась консоль

use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::Graphics::GDI::*,
    Win32::System::LibraryLoader::GetModuleHandleW,
    Win32::UI::WindowsAndMessaging::*,
};

fn main() -> Result<()> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let window_class = w!("MyMinimalNotepad");

        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance.into(),
            lpszClassName: window_class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hbrBackground: HBRUSH(COLOR_WINDOW.0 as isize),
            ..Default::default()
        };

        RegisterClassW(&wc);

        // Создаем главное окно
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            window_class,
            w!("Minimal Notepad"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT, CW_USEDEFAULT, 800, 600,
            None, None, instance, None,
        );

        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }

        Ok(())
    }
}

// Идентификатор нашего текстового поля
const ID_EDIT: i32 = 101;

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    static mut HWND_EDIT: HWND = HWND(0);

    match msg {
        WM_CREATE => {
            let instance = GetModuleHandleW(None).unwrap();
            // Создаем встроенный системный текстовый редактор
            HWND_EDIT = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("EDIT"), // Системный класс "EDIT"
                None,
                WS_CHILD | WS_VISIBLE | WS_VSCROLL | WS_HSCROLL | 
                WINDOW_STYLE(ES_MULTILINE as u32) | WINDOW_STYLE(ES_AUTOVSCROLL as u32),
                0, 0, 0, 0,
                hwnd,
                HMENU(ID_EDIT as isize),
                instance,
                None,
            );

            // Устанавливаем нормальный шрифт (Consolas), а не системный по умолчанию
            let h_font = CreateFontW(
                20, 0, 0, 0, 400, 0, 0, 0, 
                DEFAULT_CHARSET.0 as u32, 
                OUT_DEFAULT_PRECIS.0 as u32, 
                CLIP_DEFAULT_PRECIS.0 as u32, 
                CLEARTYPE_QUALITY.0 as u32, 
                VARIABLE_PITCH.0 as u32, 
                w!("Consolas")
            );
            SendMessageW(HWND_EDIT, WM_SETFONT, WPARAM(h_font.0 as usize), LPARAM(1));
            LRESULT(0)
        }
        WM_SIZE => {
            // Растягиваем текстовое поле на все окно
            let width = (lparam.0 & 0xFFFF) as i32;
            let height = ((lparam.0 >> 16) & 0xFFFF) as i32;
            MoveWindow(HWND_EDIT, 0, 0, width, height, true);
            LRESULT(0)
        }
        WM_SETFOCUS => {
            SetFocus(HWND_EDIT);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}