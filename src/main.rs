#![windows_subsystem = "windows"]

use std::fs::{self, OpenOptions};
use std::io::{Write, Seek, SeekFrom};
use std::os::windows::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::process::Command;
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

// --- ХЕЛПЕРЫ ---
fn to_wide(s: &str) -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() }
unsafe fn set_text(h: HWND, s: &str) { let _ = SetWindowTextW(h, PCWSTR(to_wide(s).as_ptr())); }
unsafe fn get_text(h: HWND) -> String {
    let len = GetWindowTextLengthW(h);
    if len == 0 { return String::new(); }
    let mut buf = vec![0u16; (len + 1) as usize];
    let actual = GetWindowTextW(h, &mut buf);
    String::from_utf16_lossy(&buf[..actual as usize])
}

// --- ГЛОБАЛЬНЫЙ СТЕЙТ ---
static mut HWND_EDIT: HWND = HWND(null_mut());
static mut HWND_REGEX_WIN: HWND = HWND(null_mut());
static mut HWND_PAT: HWND = HWND(null_mut());
static mut HWND_REP: HWND = HWND(null_mut());
static mut CURRENT_FONT_SIZE: i32 = 24;
static mut CURRENT_FONT: HFONT = HFONT(null_mut());
static mut IS_WORD_WRAP: bool = true;
static mut LAST_SEARCH_IDX: usize = 0;
static mut LAST_PATTERN: String = String::new();

// --- СТЕЙТ ДЛЯ МУСОРНИКА ---
static mut CURRENT_FILE: Option<PathBuf> = None;
static mut FILE_LOCK: Option<std::fs::File> = None;
static mut IS_DIRTY: bool = false;

const ID_EDIT: i32 = 101;
const IDM_OPEN: usize = 1001;
const IDM_SAVE: usize = 1002;
const IDM_EXIT: usize = 1003;
const IDM_WRAP: usize = 1004;
const IDM_REGEX_SHOW: usize = 1005;
const ID_BTN_REPLACE: usize = 2001;
const ID_BTN_FILTER: usize = 2002;
const ID_BTN_FIND: usize = 2003;

// Win32 Константы, которых может не быть в базовом импорте
const EM_SETLIMITTEXT: u32 = 0x00C5;
const EN_CHANGE: u16 = 0x0300;
const FILE_SHARE_READ: u32 = 1;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let dumpster_dir = "C:\\Dumpster";
    fs::create_dir_all(dumpster_dir).ok();

    unsafe {
        // МАГИЯ АВТОРЕОТКРЫТИЯ И ЛОКОВ
        if args.len() > 1 {
            // Запущен с конкретным файлом (форк)
            let path = PathBuf::from(&args[1]);
            if let Ok(f) = OpenOptions::new().read(true).write(true).create(true).share_mode(FILE_SHARE_READ).open(&path) {
                CURRENT_FILE = Some(path);
                FILE_LOCK = Some(f);
            }
        } else {
            // Запущен вслепую (Win+R). Ищем бесхозные файлы.
            if let Ok(entries) = fs::read_dir(dumpster_dir) {
                let mut first_found = false;
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("txt") {
                        // Пытаемся захватить файл (если он открыт другим окном, будет ошибка доступа)
                        if let Ok(f) = OpenOptions::new().read(true).write(true).share_mode(FILE_SHARE_READ).open(&path) {
                            if !first_found {
                                CURRENT_FILE = Some(path.clone());
                                FILE_LOCK = Some(f);
                                first_found = true;
                            } else {
                                // Нашли еще один свободный файл -> форкаем процесс для него
                                Command::new(&args[0]).arg(&path).spawn().ok();
                            }
                        }
                    }
                }
            }
            // Если все файлы заняты (или их нет), создаем новый
            if CURRENT_FILE.is_none() {
                let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
                let path = PathBuf::from(format!("{}\\dump_{}.txt", dumpster_dir, ts));
                if let Ok(f) = OpenOptions::new().read(true).write(true).create(true).share_mode(FILE_SHARE_READ).open(&path) {
                    CURRENT_FILE = Some(path);
                    FILE_LOCK = Some(f);
                }
            }
        }

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

        // Ставим имя файла в заголовок
        let title = if let Some(p) = &CURRENT_FILE {
            format!("Dumpster - {}", p.file_name().unwrap().to_string_lossy())
        } else {
            "Dumpster".to_string()
        };

        let hwnd = CreateWindowExW(WINDOW_EX_STYLE::default(), window_class, PCWSTR(to_wide(&title).as_ptr()), WS_OVERLAPPEDWINDOW | WS_VISIBLE, CW_USEDEFAULT, CW_USEDEFAULT, 900, 700, None, None, instance, None)?;

        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let mut handled = false;
            if message.message == WM_KEYDOWN {
                let ctrl = (GetKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0;
                match message.wParam.0 as u16 {
                    0x41 if ctrl => { let _ = SendMessageW(HWND_EDIT, EM_SETSEL, WPARAM(0), LPARAM(-1)); handled = true; }
                    0x46 if ctrl => { show_regex_win(hwnd); let _ = SetFocus(HWND_PAT); handled = true; }
                    0x53 if ctrl => { save_current_state(); handled = true; } // Ctrl+S принудительно
                    _ => {}
                }
            }
            if !handled { let _ = TranslateMessage(&message); DispatchMessageW(&message); }
        }
        Ok(())
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => { 
            setup_ui(hwnd); 
            // Таймер автосохранения (ID 1, каждые 2 секунды)
            SetTimer(hwnd, 1, 2000, None);
            LRESULT(0) 
        }
        WM_SIZE => {
            let (w, h) = ((lparam.0 & 0xFFFF) as i32, ((lparam.0 >> 16) & 0xFFFF) as i32);
            if !HWND_EDIT.0.is_null() { let _ = MoveWindow(HWND_EDIT, 0, 0, w, h, true); }
            LRESULT(0)
        }
        WM_SETFOCUS => { if !HWND_EDIT.0.is_null() { let _ = SetFocus(HWND_EDIT); } LRESULT(0) }
        WM_COMMAND => {
            let notify_code = (wparam.0 >> 16) as u16;
            let control_id = (wparam.0 & 0xFFFF) as u16;

            // Отслеживаем изменения текста
            if control_id == ID_EDIT as u16 && notify_code == EN_CHANGE {
                IS_DIRTY = true;
            }

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
        WM_TIMER => {
            if wparam.0 == 1 && IS_DIRTY {
                save_current_state();
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            if ((wparam.0 & 0xFFFF) as u32 & 0x0008) != 0 {
                let delta = (wparam.0 >> 16) as i16;
                CURRENT_FONT_SIZE = (CURRENT_FONT_SIZE + if delta > 0 { 2 } else { -2 }).clamp(8, 100);
                update_font();
            }
            LRESULT(0)
        }
        WM_DESTROY => { 
            save_current_state();
            // Убираем за собой пустые файлы при закрытии
            if let Some(file) = FILE_LOCK.take() {
                drop(file); // Отпускаем лок
                if let Some(path) = &CURRENT_FILE {
                    if let Ok(meta) = fs::metadata(path) {
                        if meta.len() == 0 {
                            let _ = fs::remove_file(path);
                        }
                    }
                }
            }
            PostQuitMessage(0); 
            LRESULT(0) 
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn save_current_state() {
    if let Some(file) = &mut FILE_LOCK {
        let text = get_text(HWND_EDIT);
        let _ = file.set_len(0);
        let _ = file.seek(SeekFrom::Start(0));
        let _ = file.write_all(text.as_bytes());
        IS_DIRTY = false;
    }
}

unsafe fn setup_ui(hwnd: HWND) {
    let inst = GetModuleHandleW(None).unwrap();
    if let Ok(m) = CreateMenu() {
        if let Ok(fm) = CreateMenu() {
            let _ = AppendMenuW(fm, MF_STRING, IDM_OPEN, w!("Open"));
            let _ = AppendMenuW(fm, MF_STRING, IDM_SAVE, w!("Save"));
            let _ = AppendMenuW(fm, MF_SEPARATOR, 0, PCWSTR::null());
            let _ = AppendMenuW(fm, MF_STRING, IDM_WRAP, if IS_WORD_WRAP { w!("✔ Word Wrap") } else { w!("Word Wrap") });
            let _ = AppendMenuW(fm, MF_STRING, IDM_REGEX_SHOW, w!("Regex/Find (Ctrl+F)"));
            let _ = AppendMenuW(fm, MF_STRING, IDM_EXIT, w!("Exit"));
            let _ = AppendMenuW(m, MF_POPUP, fm.0 as usize, w!("File"));
            let _ = SetMenu(hwnd, m);
        }
    }
    
    let mut st = WS_CHILD | WS_VISIBLE | WS_VSCROLL | WINDOW_STYLE(ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32 | ES_WANTRETURN as u32 | ES_NOHIDESEL as u32);
    if !IS_WORD_WRAP { st |= WS_HSCROLL | WINDOW_STYLE(ES_AUTOHSCROLL as u32); }
    
    if let Ok(h) = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("EDIT"), PCWSTR::null(), st, 0, 0, 0, 0, hwnd, HMENU(ID_EDIT as *mut _), inst, None) {
        HWND_EDIT = h;
        
        // 🔥 СНИМАЕМ ЛИМИТ СИМВОЛОВ (0 = максимум, ~4 ГБ)
        SendMessageW(HWND_EDIT, EM_SETLIMITTEXT, WPARAM(0), LPARAM(0));
        
        update_font();

        // Загружаем текст из темпофайла при старте
        if let Some(path) = &CURRENT_FILE {
            if let Ok(content) = fs::read_to_string(path) {
                set_text(HWND_EDIT, &content);
            }
        }
    }
}

// ... (Остальные функции: show_regex_win, regex_proc, find_next_auto, run_regex, update_font, toggle_word_wrap, open_file, save_file остаются БЕЗ ИЗМЕНЕНИЙ) ...

unsafe fn show_regex_win(hwnd: HWND) {
    if HWND_REGEX_WIN.0.is_null() {
        let inst = GetModuleHandleW(None).unwrap();
        HWND_REGEX_WIN = CreateWindowExW(WINDOW_EX_STYLE(WS_EX_TOOLWINDOW.0), w!("RegexToolWin"), w!("Regex & Find"), WS_CAPTION | WS_SYSMENU, CW_USEDEFAULT, CW_USEDEFAULT, 350, 270, hwnd, None, inst, None).unwrap();
        let _ = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("STATIC"), w!("Pattern:"), WS_CHILD | WS_VISIBLE, 10, 10, 300, 20, HWND_REGEX_WIN, None, inst, None);
        HWND_PAT = CreateWindowExW(WINDOW_EX_STYLE(WS_EX_CLIENTEDGE.0), w!("EDIT"), PCWSTR::null(), WS_CHILD | WS_VISIBLE | WINDOW_STYLE(ES_AUTOHSCROLL as u32), 10, 30, 310, 25, HWND_REGEX_WIN, None, inst, None).unwrap();
        let _ = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("STATIC"), w!("Replace with:"), WS_CHILD | WS_VISIBLE, 10, 60, 300, 20, HWND_REGEX_WIN, None, inst, None);
        HWND_REP = CreateWindowExW(WINDOW_EX_STYLE(WS_EX_CLIENTEDGE.0), w!("EDIT"), PCWSTR::null(), WS_CHILD | WS_VISIBLE | WINDOW_STYLE(ES_AUTOHSCROLL as u32), 10, 80, 310, 25, HWND_REGEX_WIN, None, inst, None).unwrap();
        let _ = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("BUTTON"), w!("Find Next (Enter)"), WS_CHILD | WS_VISIBLE, 10, 130, 310, 30, HWND_REGEX_WIN, HMENU(ID_BTN_FIND as *mut _), inst, None);
        let _ = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("BUTTON"), w!("Replace All"), WS_CHILD | WS_VISIBLE, 10, 165, 150, 30, HWND_REGEX_WIN, HMENU(ID_BTN_REPLACE as *mut _), inst, None);
        let _ = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("BUTTON"), w!("Filter Lines"), WS_CHILD | WS_VISIBLE, 170, 165, 150, 30, HWND_REGEX_WIN, HMENU(ID_BTN_FILTER as *mut _), inst, None);
    }
    let _ = ShowWindow(HWND_REGEX_WIN, SW_SHOW);
}

unsafe extern "system" fn regex_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            match wparam.0 {
                ID_BTN_REPLACE => run_regex(false),
                ID_BTN_FILTER => run_regex(true),
                ID_BTN_FIND => find_next_auto(),
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLOSE => { let _ = ShowWindow(hwnd, SW_HIDE); LRESULT(0) }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn find_next_auto() {
    let (pat, content) = (get_text(HWND_PAT), get_text(HWND_EDIT));
    if pat.is_empty() { return; }
    if pat != LAST_PATTERN { LAST_PATTERN = pat.clone(); LAST_SEARCH_IDX = 0; }
    if let Ok(re) = Regex::new(&pat) {
        if let Some(m) = re.find_at(&content, LAST_SEARCH_IDX).or_else(|| re.find(&content)) {
            let start_char = content[..m.start()].encode_utf16().count();
            let end_char = content[..m.end()].encode_utf16().count();
            let _ = SendMessageW(HWND_EDIT, EM_SETSEL, WPARAM(start_char), LPARAM(end_char as isize));
            let _ = SendMessageW(HWND_EDIT, EM_SCROLLCARET, WPARAM(0), LPARAM(0));
            LAST_SEARCH_IDX = m.end();
        } else { LAST_SEARCH_IDX = 0; }
    }
}

unsafe fn run_regex(filter_mode: bool) {
    let (pat, rep, content) = (get_text(HWND_PAT), get_text(HWND_REP), get_text(HWND_EDIT));
    if let Ok(re) = Regex::new(&pat) {
        let new = if filter_mode { content.lines().filter(|l| re.is_match(l)).collect::<Vec<_>>().join("\r\n") } 
                  else { re.replace_all(&content, rep.as_str()).to_string() };
        set_text(HWND_EDIT, &new);
    }
}

unsafe fn update_font() {
    if !HWND_EDIT.0.is_null() {
        if !CURRENT_FONT.0.is_null() { let _ = DeleteObject(CURRENT_FONT); }
        CURRENT_FONT = CreateFontW(CURRENT_FONT_SIZE, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 4, 0, w!("Consolas"));
        let _ = SendMessageW(HWND_EDIT, WM_SETFONT, WPARAM(CURRENT_FONT.0 as usize), LPARAM(1));
    }
}

unsafe fn toggle_word_wrap(hwnd: HWND) {
    IS_WORD_WRAP = !IS_WORD_WRAP;
    let txt = get_text(HWND_EDIT);
    let _ = DestroyWindow(HWND_EDIT);
    setup_ui(hwnd);
    set_text(HWND_EDIT, &txt);
    let mut r = RECT::default();
    let _ = GetClientRect(hwnd, &mut r);
    let _ = MoveWindow(HWND_EDIT, 0, 0, r.right, r.bottom, true);
    let _ = SetFocus(HWND_EDIT);
}

unsafe fn open_file(hwnd: HWND) {
    let mut f = [0u16; 260];
    let mut ofn = OPENFILENAMEW { lStructSize: 152, hwndOwner: hwnd, lpstrFile: PWSTR(f.as_mut_ptr()), nMaxFile: 260, lpstrFilter: w!("Text Files\0*.txt\0All Files\0*.*\0"), nFilterIndex: 1, Flags: OFN_PATHMUSTEXIST | OFN_FILEMUSTEXIST, ..Default::default() };
    if GetOpenFileNameW(&mut ofn).as_bool() {
        if let Ok(c) = fs::read_to_string(String::from_utf16_lossy(&f).trim_matches(char::from(0))) { set_text(HWND_EDIT, &c); }
    }
}

unsafe fn save_file(hwnd: HWND) {
    let mut f = [0u16; 260];
    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: hwnd,
        lpstrFile: PWSTR(f.as_mut_ptr()),
        nMaxFile: 260,
        lpstrFilter: w!("Text Files\0*.txt\0All Files\0*.*\0"),
        nFilterIndex: 1,
        
        lpstrDefExt: w!("txt"), 
        
        Flags: OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST,
        ..Default::default()
    };

    if GetSaveFileNameW(&mut ofn).as_bool() {
        let len = f.iter().position(|&c| c == 0).unwrap_or(f.len());
        let path = String::from_utf16_lossy(&f[..len]);
        
        let _ = fs::write(path, get_text(HWND_EDIT));
    }
}
