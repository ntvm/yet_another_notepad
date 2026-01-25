#![windows_subsystem = "windows"]

use std::fs;
use std::ptr::null_mut;
use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::Graphics::Gdi::*,
    Win32::System::LibraryLoader::GetModuleHandleW,
    Win32::UI::WindowsAndMessaging::*,
    Win32::UI::Input::KeyboardAndMouse::*,
    Win32::UI::Controls::Dialogs::*,
    Win32::UI::Controls::*,
};

// Глобальное состояние
static mut HWND_EDIT: HWND = HWND(null_mut());
static mut CURRENT_FONT_SIZE: i32 = 24;
static mut CURRENT_FONT: HFONT = HFONT(null_mut());
static mut IS_WORD_WRAP: bool = false; // Состояние переноса слов

// Константы
const ID_EDIT: i32 = 101;
const IDM_OPEN: usize = 1001;
const IDM_SAVE: usize = 1002;
const IDM_EXIT: usize = 1003;
const IDM_WRAP: usize = 1004; // Новая кнопка
const MK_CONTROL: u32 = 0x0008; 

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

        let _hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            window_class,
            w!("Minimal Notepad"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT, CW_USEDEFAULT, 800, 600,
            None, None, instance, None,
        )?;

        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }

        Ok(())
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            setup_ui(hwnd);
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
            if !HWND_EDIT.0.is_null() { let _ = SetFocus(HWND_EDIT); }
            LRESULT(0)
        }

        WM_COMMAND => {
            let id = wparam.0 & 0xFFFF;
            match id as usize {
                IDM_OPEN => { open_file(hwnd); }
                IDM_SAVE => { save_file(hwnd); }
                IDM_EXIT => { let _ = PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0)); }
                IDM_WRAP => { toggle_word_wrap(hwnd); }
                _ => {}
            }
            LRESULT(0)
        }

        WM_MOUSEWHEEL => {
            let keys = (wparam.0 & 0xFFFF) as u32;
            if (keys & MK_CONTROL) != 0 {
                let delta = (wparam.0 >> 16) as i16;
                if delta > 0 { CURRENT_FONT_SIZE += 2; } else { CURRENT_FONT_SIZE -= 2; }
                if CURRENT_FONT_SIZE < 8 { CURRENT_FONT_SIZE = 8; }
                if CURRENT_FONT_SIZE > 100 { CURRENT_FONT_SIZE = 100; }
                update_font();
            }
            LRESULT(0)
        }

        WM_DESTROY => {
            if !CURRENT_FONT.0.is_null() { let _ = DeleteObject(CURRENT_FONT); }
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn setup_ui(hwnd: HWND) {
    let instance = GetModuleHandleW(None).unwrap();
    
    // Меню
    if let Ok(h_menu) = CreateMenu() {
        if let Ok(h_file_menu) = CreateMenu() {
            let _ = AppendMenuW(h_file_menu, MF_STRING, IDM_OPEN, w!("Open"));
            let _ = AppendMenuW(h_file_menu, MF_STRING, IDM_SAVE, w!("Save"));
            let _ = AppendMenuW(h_file_menu, MF_SEPARATOR, 0, PCWSTR::null());
            let _ = AppendMenuW(h_file_menu, MF_STRING, IDM_WRAP, if IS_WORD_WRAP { w!("✔ Word Wrap") } else { w!("Word Wrap") });
            let _ = AppendMenuW(h_file_menu, MF_STRING, IDM_EXIT, w!("Exit"));
            let _ = AppendMenuW(h_menu, MF_POPUP, h_file_menu.0 as usize, w!("File"));
            let _ = SetMenu(hwnd, h_menu);
        }
    }

    // Стили EDIT
    // Если Wrap выключен — добавляем горизонтальный скролл и авто-скролл по горизонтали
    let mut style = WS_CHILD | WS_VISIBLE | WS_VSCROLL | WINDOW_STYLE(ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32 | ES_WANTRETURN as u32);
    if !IS_WORD_WRAP {
        style |= WS_HSCROLL | WINDOW_STYLE(ES_AUTOHSCROLL as u32);
    }

    HWND_EDIT = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        w!("EDIT"),
        None,
        style,
        0, 0, 0, 0,
        hwnd,
        HMENU(ID_EDIT as *mut core::ffi::c_void),
        instance,
        None,
    ).unwrap();

    update_font();
}

unsafe fn toggle_word_wrap(hwnd: HWND) {
    IS_WORD_WRAP = !IS_WORD_WRAP;

    // 1. Сохраняем текст
    let len = GetWindowTextLengthW(HWND_EDIT);
    let mut buffer = vec![0u16; (len + 1) as usize];
    GetWindowTextW(HWND_EDIT, &mut buffer);

    // 2. Удаляем старое окно
    let _ = DestroyWindow(HWND_EDIT);

    // 3. Создаем новое с новыми стилями
    setup_ui(hwnd);

    // 4. Возвращаем текст
    let _ = SetWindowTextW(HWND_EDIT, PCWSTR(buffer.as_ptr()));

    // 5. Подгоняем размер под текущее окно
    let mut rect = RECT::default();
    let _ = GetClientRect(hwnd, &mut rect);
    let _ = MoveWindow(HWND_EDIT, 0, 0, rect.right, rect.bottom, true);
    let _ = SetFocus(HWND_EDIT);
}

unsafe fn update_font() {
    if !HWND_EDIT.0.is_null() {
        if !CURRENT_FONT.0.is_null() { let _ = DeleteObject(CURRENT_FONT); }
        let h_font = CreateFontW(
            CURRENT_FONT_SIZE, 0, 0, 0, 400, 0, 0, 0, 
            DEFAULT_CHARSET.0 as u32, OUT_DEFAULT_PRECIS.0 as u32, 
            CLIP_DEFAULT_PRECIS.0 as u32, CLEARTYPE_QUALITY.0 as u32, 
            VARIABLE_PITCH.0 as u32, w!("Consolas")
        ).unwrap();
        CURRENT_FONT = h_font;
        let _ = SendMessageW(HWND_EDIT, WM_SETFONT, WPARAM(h_font.0 as usize), LPARAM(1));
    }
}

// Функции open_file и save_file остаются без изменений...
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
        let path = path.trim_matches(char::from(0));
        if let Ok(content) = fs::read_to_string(path) {
            let mut wide_content: Vec<u16> = content.encode_utf16().collect();
            wide_content.push(0);
            let _ = SetWindowTextW(HWND_EDIT, PCWSTR(wide_content.as_ptr()));
        }
    }
}

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
        let len = GetWindowTextLengthW(HWND_EDIT);
        if len > 0 {
            let mut buffer = vec![0u16; (len + 1) as usize];
            let actual_len = GetWindowTextW(HWND_EDIT, &mut buffer);
            if let Ok(text) = String::from_utf16(&buffer[..actual_len as usize]) {
                let _ = fs::write(path, text);
            }
        } else { let _ = fs::write(path, ""); }
    }
}
