#![windows_subsystem = "windows"]

use std::fs;
use std::ptr::null_mut;
use regex::Regex;
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

static mut HWND_EDIT: HWND = HWND(null_mut());
static mut HWND_REGEX_WIN: HWND = HWND(null_mut());
static mut HWND_PAT: HWND = HWND(null_mut());
static mut HWND_REP: HWND = HWND(null_mut());

static mut CURRENT_FONT_SIZE: i32 = 24;
static mut CURRENT_FONT: HFONT = HFONT(null_mut());
static mut IS_WORD_WRAP: bool = true;
static mut LAST_SEARCH_IDX: usize = 0; // Для "Find Next"

const ID_EDIT: i32 = 101;
const IDM_OPEN: usize = 1001;
const IDM_SAVE: usize = 1002;
const IDM_EXIT: usize = 1003;
const IDM_WRAP: usize = 1004;
const IDM_REGEX_SHOW: usize = 1005;
const ID_BTN_REPLACE: usize = 2001;
const ID_BTN_FILTER: usize = 2002;
const ID_BTN_FIND: usize = 2003; // Новая кнопка
const MK_CONTROL: u32 = 0x0008;

fn main() -> Result<()> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let window_class = w!("MyMinimalNotepad");
        let regex_class = w!("RegexToolWin");

        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance.into(),
            lpszClassName: window_class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as *mut _),
            ..Default::default()
        };
        RegisterClassW(&wc);

        let rc = WNDCLASSW {
            lpfnWndProc: Some(regex_proc),
            hInstance: instance.into(),
            lpszClassName: regex_class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hbrBackground: HBRUSH((COLOR_BTNFACE.0 + 1) as *mut _),
            ..Default::default()
        };
        RegisterClassW(&rc);

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            window_class,
            w!("Vibecoded Notepad + Regex"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT, CW_USEDEFAULT, 900, 700,
            None, None, instance, None,
        )?;

        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let mut handled = false;
            if message.message == WM_KEYDOWN {
                let ctrl = (GetKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0;
                match message.wParam.0 as u16 {
                    0x41 if ctrl => { // Ctrl+A
                        if !HWND_EDIT.0.is_null() {
                            let _ = SendMessageW(HWND_EDIT, EM_SETSEL, WPARAM(0), LPARAM(-1));
                            handled = true;
                        }
                    }
                    0x46 if ctrl => { // Ctrl+F
                        show_regex_win(hwnd);
                        let _ = SetFocus(HWND_PAT);
                        handled = true;
                    }
                    _ => {}
                }
            }

            if !handled {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        Ok(())
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => { setup_ui(hwnd); LRESULT(0) }
        WM_SIZE => {
            let width = (lparam.0 & 0xFFFF) as i32;
            let height = ((lparam.0 >> 16) & 0xFFFF) as i32;
            if !HWND_EDIT.0.is_null() { let _ = MoveWindow(HWND_EDIT, 0, 0, width, height, true); }
            LRESULT(0)
        }
        WM_SETFOCUS => { if !HWND_EDIT.0.is_null() { let _ = SetFocus(HWND_EDIT); } LRESULT(0) }
        WM_COMMAND => {
            match wparam.0 {
                IDM_OPEN => open_file(hwnd),
                IDM_SAVE => save_file(hwnd),
                IDM_WRAP => toggle_word_wrap(hwnd),
                IDM_REGEX_SHOW => show_regex_win(hwnd),
                IDM_EXIT => { let _ = PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0)); }
                _ => {}
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            if ((wparam.0 & 0xFFFF) as u32 & MK_CONTROL) != 0 {
                let delta = (wparam.0 >> 16) as i16;
                if delta > 0 { CURRENT_FONT_SIZE += 2; } else { CURRENT_FONT_SIZE -= 2; }
                CURRENT_FONT_SIZE = CURRENT_FONT_SIZE.clamp(8, 100);
                update_font();
            }
            LRESULT(0)
        }
        WM_DESTROY => { PostQuitMessage(0); LRESULT(0) }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn setup_ui(hwnd: HWND) {
    let instance = GetModuleHandleW(None).unwrap();
    if let Ok(h_menu) = CreateMenu() {
        if let Ok(h_file_menu) = CreateMenu() {
            let _ = AppendMenuW(h_file_menu, MF_STRING, IDM_OPEN, w!("Open"));
            let _ = AppendMenuW(h_file_menu, MF_STRING, IDM_SAVE, w!("Save"));
            let _ = AppendMenuW(h_file_menu, MF_SEPARATOR, 0, PCWSTR::null());
            let _ = AppendMenuW(h_file_menu, MF_STRING, IDM_WRAP, if IS_WORD_WRAP { w!("✔ Word Wrap") } else { w!("Word Wrap") });
            let _ = AppendMenuW(h_file_menu, MF_STRING, IDM_REGEX_SHOW, w!("Regex/Find (Ctrl+F)"));
            let _ = AppendMenuW(h_file_menu, MF_STRING, IDM_EXIT, w!("Exit"));
            let _ = AppendMenuW(h_menu, MF_POPUP, h_file_menu.0 as usize, w!("File"));
            let _ = SetMenu(hwnd, h_menu);
        }
    }
    let mut style = WS_CHILD | WS_VISIBLE | WS_VSCROLL | WINDOW_STYLE(ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32 | ES_WANTRETURN as u32);
    if !IS_WORD_WRAP { style |= WS_HSCROLL | WINDOW_STYLE(ES_AUTOHSCROLL as u32); }
    if let Ok(h) = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("EDIT"), PCWSTR::null(), style, 0, 0, 0, 0, hwnd, HMENU(ID_EDIT as *mut _), instance, None) {
        HWND_EDIT = h;
        update_font();
    }
}

unsafe fn show_regex_win(hwnd: HWND) {
    if HWND_REGEX_WIN.0.is_null() {
        let instance = GetModuleHandleW(None).unwrap();
        HWND_REGEX_WIN = CreateWindowExW(WINDOW_EX_STYLE(WS_EX_TOOLWINDOW.0), w!("RegexToolWin"), w!("Regex & Find"), WS_CAPTION | WS_SYSMENU, CW_USEDEFAULT, CW_USEDEFAULT, 350, 215, hwnd, None, instance, None).unwrap();
        let _ = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("STATIC"), w!("Pattern:"), WS_CHILD | WS_VISIBLE, 10, 10, 300, 20, HWND_REGEX_WIN, None, instance, None);
        HWND_PAT = CreateWindowExW(WINDOW_EX_STYLE(WS_EX_CLIENTEDGE.0), w!("EDIT"), PCWSTR::null(), WS_CHILD | WS_VISIBLE | WINDOW_STYLE(ES_AUTOHSCROLL as u32), 10, 30, 310, 25, HWND_REGEX_WIN, None, instance, None).unwrap();
        let _ = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("STATIC"), w!("Replace with:"), WS_CHILD | WS_VISIBLE, 10, 60, 300, 20, HWND_REGEX_WIN, None, instance, None);
        HWND_REP = CreateWindowExW(WINDOW_EX_STYLE(WS_EX_CLIENTEDGE.0), w!("EDIT"), PCWSTR::null(), WS_CHILD | WS_VISIBLE | WINDOW_STYLE(ES_AUTOHSCROLL as u32), 10, 80, 310, 25, HWND_REGEX_WIN, None, instance, None).unwrap();
        
        let _ = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("STATIC"), w!("(Press Enter to search)"), WS_CHILD | WS_VISIBLE, 10, 110, 310, 15, HWND_REGEX_WIN, None, instance, None);
        let _ = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("BUTTON"), w!("Find Next (Enter)"), WS_CHILD | WS_VISIBLE, 10, 130, 310, 30, HWND_REGEX_WIN, HMENU(ID_BTN_FIND as *mut _), instance, None);
        let _ = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("BUTTON"), w!("Replace All"), WS_CHILD | WS_VISIBLE, 10, 165, 150, 30, HWND_REGEX_WIN, HMENU(ID_BTN_REPLACE as *mut _), instance, None);
        let _ = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("BUTTON"), w!("Filter Lines"), WS_CHILD | WS_VISIBLE, 170, 165, 150, 30, HWND_REGEX_WIN, HMENU(ID_BTN_FILTER as *mut _), instance, None);
    }
    let _ = ShowWindow(HWND_REGEX_WIN, SW_SHOW);
}

unsafe extern "system" fn regex_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            match wparam.0 {
                ID_BTN_REPLACE => run_regex(false),
                ID_BTN_FILTER => run_regex(true),
                ID_BTN_FIND => find_next(),
                _ => {}
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let key_code = wparam.0 as u16;
            // Обработка Enter в поле pattern для поиска
            if key_code == VK_RETURN.0 as u16 {
                if GetFocus() == HWND_PAT || GetFocus() == HWND_REP {
                    find_next_auto();
                    return LRESULT(0);
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_CLOSE => { let _ = ShowWindow(hwnd, SW_HIDE); LRESULT(0) }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn find_next() {
    LAST_SEARCH_IDX = 0; // Начинаем с начала при нажатии кнопки
    find_next_auto();
}

unsafe fn find_next_auto() {
    let pat = get_text(HWND_PAT);
    let content = get_text(HWND_EDIT);
    if pat.is_empty() {
        return;
    }
    if let Ok(re) = Regex::new(&pat) {
        // Ищем начиная с LAST_SEARCH_IDX
        if let Some(m) = re.find_at(&content, LAST_SEARCH_IDX).or_else(|| re.find(&content)) {
            let start = m.start();
            let end = m.end();
            
            // Конвертируем байтовые смещения Rust в символьные смещения Win32 (UTF-16)
            let char_start = content[..start].encode_utf16().count();
            let char_end = content[..end].encode_utf16().count();
            
            // Выделяем найденный текст
            let _ = SendMessageW(HWND_EDIT, EM_SETSEL, WPARAM(char_start), LPARAM(char_end as isize));
            let _ = SendMessageW(HWND_EDIT, EM_SCROLLCARET, WPARAM(0), LPARAM(0));
            
            LAST_SEARCH_IDX = end; // Запоминаем для следующего раза
        } else {
            LAST_SEARCH_IDX = 0; // Если нет совпадений, начинаем с начала
        }
    }
}

unsafe fn run_regex(filter_mode: bool) {
    let pat = get_text(HWND_PAT);
    let rep = get_text(HWND_REP);
    let content = get_text(HWND_EDIT);
    if let Ok(re) = Regex::new(&pat) {
        let new_content = if filter_mode {
            content.lines().filter(|line| re.is_match(line)).collect::<Vec<_>>().join("\r\n")
        } else {
            re.replace_all(&content, rep.as_str()).to_string()
        };
        let mut wide: Vec<u16> = new_content.encode_utf16().collect();
        wide.push(0);
        let _ = SetWindowTextW(HWND_EDIT, PCWSTR(wide.as_ptr()));
    }
}

unsafe fn get_text(h: HWND) -> String {
    let len = GetWindowTextLengthW(h);
    if len == 0 { return String::new(); }
    let mut buf = vec![0u16; (len + 1) as usize];
    let actual = GetWindowTextW(h, &mut buf);
    String::from_utf16_lossy(&buf[..actual as usize])
}

unsafe fn update_font() {
    if !HWND_EDIT.0.is_null() {
        if !CURRENT_FONT.0.is_null() { let _ = DeleteObject(CURRENT_FONT); }
        let h_font = CreateFontW(CURRENT_FONT_SIZE, 0, 0, 0, 400, 0, 0, 0, DEFAULT_CHARSET.0 as u32, OUT_DEFAULT_PRECIS.0 as u32, CLIP_DEFAULT_PRECIS.0 as u32, CLEARTYPE_QUALITY.0 as u32, VARIABLE_PITCH.0 as u32, w!("Consolas"));
        CURRENT_FONT = h_font;
        let _ = SendMessageW(HWND_EDIT, WM_SETFONT, WPARAM(h_font.0 as usize), LPARAM(1));
    }
}

unsafe fn toggle_word_wrap(hwnd: HWND) {
    IS_WORD_WRAP = !IS_WORD_WRAP;
    let content = get_text(HWND_EDIT);
    let _ = DestroyWindow(HWND_EDIT);
    setup_ui(hwnd);
    let mut wide: Vec<u16> = content.encode_utf16().collect();
    wide.push(0);
    let _ = SetWindowTextW(HWND_EDIT, PCWSTR(wide.as_ptr()));
    let mut rect = RECT::default();
    let _ = GetClientRect(hwnd, &mut rect);
    let _ = MoveWindow(HWND_EDIT, 0, 0, rect.right, rect.bottom, true);
    let _ = SetFocus(HWND_EDIT);
}

unsafe fn open_file(hwnd: HWND) {
    let mut filename = [0u16; 260];
    let mut ofn = OPENFILENAMEW { lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32, hwndOwner: hwnd, lpstrFile: PWSTR(filename.as_mut_ptr()), nMaxFile: 260, lpstrFilter: w!("Text Files\0*.txt\0All Files\0*.*\0"), nFilterIndex: 1, Flags: OFN_PATHMUSTEXIST | OFN_FILEMUSTEXIST, ..Default::default() };
    if GetOpenFileNameW(&mut ofn).as_bool() {
        let path = String::from_utf16_lossy(&filename);
        let path = path.trim_matches(char::from(0));
        if let Ok(content) = fs::read_to_string(path) {
            let mut wide: Vec<u16> = content.encode_utf16().collect();
            wide.push(0);
            let _ = SetWindowTextW(HWND_EDIT, PCWSTR(wide.as_ptr()));
        }
    }
}

unsafe fn save_file(hwnd: HWND) {
    let mut filename = [0u16; 260];
    let mut ofn = OPENFILENAMEW { lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32, hwndOwner: hwnd, lpstrFile: PWSTR(filename.as_mut_ptr()), nMaxFile: 260, lpstrFilter: w!("Text Files\0*.txt\0All Files\0*.*\0"), nFilterIndex: 1, Flags: OFN_OVERWRITEPROMPT, ..Default::default() };
    if GetSaveFileNameW(&mut ofn).as_bool() {
        let path = String::from_utf16_lossy(&filename);
        let path = path.trim_matches(char::from(0));
        let content = get_text(HWND_EDIT);
        let _ = fs::write(path, content);
    }
}