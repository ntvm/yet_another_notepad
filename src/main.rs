#![windows_subsystem = "windows"]

use std::fs;
use std::ptr::null_mut;
use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::Graphics::Gdi::*,
    Win32::System::LibraryLoader::GetModuleHandleW,
    Win32::UI::WindowsAndMessaging::*,
    Win32::UI::Controls::*,
    Win32::UI::Input::KeyboardAndMouse::*,
    Win32::UI::Controls::Dialogs::*,
};

// Глобальное состояние
static mut HWND_EDIT: HWND = HWND(null_mut());
static mut CURRENT_FONT_SIZE: i32 = 24; // Размер шрифта по умолчанию
static mut CURRENT_FONT: HFONT = HFONT(null_mut());

// ID для меню
const ID_EDIT: i32 = 101;
const IDM_OPEN: usize = 1001;
const IDM_SAVE: usize = 1002;
const IDM_EXIT: usize = 1003;

fn main() -> Result<()> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let window_class = w!("MyMinimalNotepad");

        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance.into(),
            lpszClassName: window_class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as *mut core::ffi::c_void),
            ..Default::default()
        };

        RegisterClassW(&wc);

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            window_class,
            w!("Minimal Notepad"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT, CW_USEDEFAULT, 800, 600,
            None, None, instance, None,
        )?;

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
            
            // 1. Создаем Меню
            let h_menu = CreateMenu().unwrap();
            let h_file_menu = CreateMenu().unwrap();
            
            AppendMenuW(h_file_menu, MF_STRING, IDM_OPEN, w!("Open"));
            AppendMenuW(h_file_menu, MF_STRING, IDM_SAVE, w!("Save"));
            AppendMenuW(h_file_menu, MF_SEPARATOR, 0, PCWSTR::null());
            AppendMenuW(h_file_menu, MF_STRING, IDM_EXIT, w!("Exit"));
            
            AppendMenuW(h_menu, MF_POPUP, h_file_menu.0 as usize, w!("File"));
            SetMenu(hwnd, h_menu);

            // 2. Создаем поле редактирования
            let edit_res = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("EDIT"),
                None,
                WS_CHILD | WS_VISIBLE | WS_VSCROLL | WS_HSCROLL | 
                WINDOW_STYLE(ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32 | ES_WANTRETURN as u32),
                0, 0, 0, 0,
                hwnd,
                HMENU(ID_EDIT as *mut core::ffi::c_void),
                instance,
                None,
            );

            if let Ok(h) = edit_res {
                HWND_EDIT = h;
                update_font(); // Устанавливаем начальный шрифт
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

        // --- ОБРАБОТКА МЕНЮ ---
        WM_COMMAND => {
            let id = wparam.0 & 0xFFFF; // Low word is ID
            match id {
                IDM_OPEN => { open_file(hwnd); }
                IDM_SAVE => { save_file(hwnd); }
                IDM_EXIT => { PostQuitMessage(0); }
                _ => {}
            }
            LRESULT(0)
        }

        // --- ЗУМ (Ctrl + Колесо) ---
        WM_MOUSEWHEEL => {
            // Проверяем, зажат ли Ctrl (MK_CONTROL = 0x0008)
            let keys = (wparam.0 & 0xFFFF) as u16;
            if (keys & MK_CONTROL as u16) != 0 {
                // Получаем направление прокрутки (High word)
                let delta = (wparam.0 >> 16) as i16;
                
                if delta > 0 {
                    CURRENT_FONT_SIZE += 2;
                } else {
                    CURRENT_FONT_SIZE -= 2;
                }

                // Ограничиваем размер
                if CURRENT_FONT_SIZE < 8 { CURRENT_FONT_SIZE = 8; }
                if CURRENT_FONT_SIZE > 100 { CURRENT_FONT_SIZE = 100; }

                update_font();
            }
            LRESULT(0)
        }

        WM_DESTROY => {
            if !CURRENT_FONT.0.is_null() {
                DeleteObject(CURRENT_FONT.into());
            }
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

// Функция обновления шрифта
unsafe fn update_font() {
    if !HWND_EDIT.0.is_null() {
        // Удаляем старый шрифт, чтобы не текла память
        if !CURRENT_FONT.0.is_null() {
            DeleteObject(CURRENT_FONT.into());
        }

        let h_font = CreateFontW(
            CURRENT_FONT_SIZE, 0, 0, 0, 400, 0, 0, 0, 
            DEFAULT_CHARSET.0 as u32, 
            OUT_DEFAULT_PRECIS.0 as u32, 
            CLIP_DEFAULT_PRECIS.0 as u32, 
            CLEARTYPE_QUALITY.0 as u32, 
            VARIABLE_PITCH.0 as u32, 
            w!("Consolas")
        );
        
        CURRENT_FONT = h_font;
        SendMessageW(HWND_EDIT, WM_SETFONT, WPARAM(h_font.0 as usize), LPARAM(1));
    }
}

// Логика открытия файла
unsafe fn open_file(hwnd: HWND) {
    let mut filename = [0u16; 260];
    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: hwnd,
        lpstrFile: PWSTR(filename.as_mut_ptr()),
        nMaxFile: 260,
        lpstrFilter: w!("Text Files\0*.txt\0All Files\0*.*\0"),
        nFilterIndex: 1,
        Flags: OFN_PATHMUSTEXIST | OFN_FILEMUSTEXIST,
        ..Default::default()
    };

    if GetOpenFileNameW(&mut ofn).as_bool() {
        let path = String::from_utf16_lossy(&filename);
        let path = path.trim_matches(char::from(0)); // Убираем лишние нули
        
        if let Ok(content) = fs::read_to_string(path) {
            // Конвертируем в UTF-16 для Windows
            let mut wide_content: Vec<u16> = content.encode_utf16().collect();
            wide_content.push(0);
            
            SetWindowTextW(HWND_EDIT, PCWSTR(wide_content.as_ptr()));
        }
    }
}

// Логика сохранения файла
unsafe fn save_file(hwnd: HWND) {
    let mut filename = [0u16; 260];
    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: hwnd,
        lpstrFile: PWSTR(filename.as_mut_ptr()),
        nMaxFile: 260,
        lpstrFilter: w!("Text Files\0*.txt\0All Files\0*.*\0"),
        nFilterIndex: 1,
        Flags: OFN_OVERWRITEPROMPT,
        ..Default::default()
    };

    if GetSaveFileNameW(&mut ofn).as_bool() {
        let path = String::from_utf16_lossy(&filename);
        let path = path.trim_matches(char::from(0));

        // Получаем текст из окна
        let len = GetWindowTextLengthW(HWND_EDIT);
        if len > 0 {
            let mut buffer = vec![0u16; (len + 1) as usize];
            GetWindowTextW(HWND_EDIT, &mut buffer);
            
            // Убираем последний ноль и сохраняем
            if let Ok(text) = String::from_utf16(&buffer[..len as usize]) {
                let _ = fs::write(path, text);
            }
        } else {
            let _ = fs::write(path, "");
        }
    }
}