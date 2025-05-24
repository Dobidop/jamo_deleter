use std::{ptr, thread, time::Duration};
use unicode_normalization::UnicodeNormalization;

use windows_sys::Win32::System::DataExchange::{
    OpenClipboard, CloseClipboard,
    GetClipboardData, SetClipboardData,
    EmptyClipboard,
};

use windows_sys::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalUnlock,
    GHND,
};

use windows_sys::Win32::System::Ole::CF_UNICODETEXT;

use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetMessageW, TranslateMessage, DispatchMessageW,
    MSG, WM_HOTKEY, GetForegroundWindow,
};

use windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT_0;

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, MOD_SHIFT,
    SendInput, INPUT, KEYBDINPUT, INPUT_KEYBOARD,
    KEYEVENTF_KEYUP, VK_BACK, VK_SHIFT, VK_CONTROL, VK_LEFT,
};

use std::collections::HashMap;

// Compatibility jamo → physical key (as Windows VK codes)
fn jamo_to_keys(j: char) -> Option<Vec<u16>> {
    let map: HashMap<char, Vec<u16>> = [
        ('ㄱ', vec![0x52]), // r
        ('ㄴ', vec![0x53]), // s
        ('ㄷ', vec![0x45]), // e
        ('ㄹ', vec![0x46]), // f
        ('ㅁ', vec![0x41]), // a
        ('ㅂ', vec![0x51]), // q
        ('ㅅ', vec![0x54]), // t
        ('ㅇ', vec![0x44]), // d
        ('ㅈ', vec![0x57]), // w
        ('ㅊ', vec![0x43]), // c
        ('ㅋ', vec![0x5A]), // z
        ('ㅌ', vec![0x58]), // x
        ('ㅍ', vec![0x56]), // v
        ('ㅎ', vec![0x47]), // g
        ('ㅏ', vec![0x4B]), // k
        ('ㅑ', vec![0x49]), // i
        ('ㅓ', vec![0x4A]), // j
        ('ㅕ', vec![0x55]), // u
        ('ㅗ', vec![0x48]), // h
        ('ㅛ', vec![0x59]), // y
        ('ㅜ', vec![0x4E]), // n
        ('ㅠ', vec![0x42]), // b
        ('ㅡ', vec![0x4D]), // m
        ('ㅣ', vec![0x4C]), // l
    ].iter().cloned().collect();

    map.get(&j).cloned()
}

// Double consonants require Shift + base key
fn jamo_to_double_keys(j: char) -> Option<u16> {
    match j {
        'ㄲ' => Some(0x52), // Shift + r
        'ㄸ' => Some(0x45), // Shift + e
        'ㅃ' => Some(0x51), // Shift + q
        'ㅆ' => Some(0x54), // Shift + t
        'ㅉ' => Some(0x57), // Shift + w
        _ => None,
    }
}

// Check if a jamo is a double consonant
fn is_double_consonant(j: char) -> bool {
    matches!(j, 'ㄲ' | 'ㄸ' | 'ㅃ' | 'ㅆ' | 'ㅉ')
}

// Compound jongseong split map: U+11xx → two codepoints
static JONGSEONG_SPLIT: once_cell::sync::Lazy<std::collections::HashMap<u32, [u32; 2]>> =
    once_cell::sync::Lazy::new(|| {
        let mut m = std::collections::HashMap::new();
        m.insert(0x11AA, [0x11A8, 0x11BA]); // ㄳ → ㄱ,ㅅ
        m.insert(0x11AC, [0x11AB, 0x11BD]); // ㄵ → ㄴ,ㅈ
        m.insert(0x11AD, [0x11AB, 0x11C2]); // ㄶ → ㄴ,ㅎ
        m.insert(0x11B0, [0x11AF, 0x11A8]); // ㄺ → ㄹ,ㄱ
        m.insert(0x11B1, [0x11AF, 0x11B7]); // ㄻ → ㄹ,ㅁ
        m.insert(0x11B2, [0x11AF, 0x11B8]); // ㄼ → ㄹ,ㅂ
        m.insert(0x11B3, [0x11AF, 0x11BA]); // ㄽ → ㄹ,ㅅ
        m.insert(0x11B4, [0x11AF, 0x11BC]); // ㄾ → ㄹ,ㅌ
        m.insert(0x11B5, [0x11AF, 0x11BD]); // ㄿ → ㄹ,ㅍ
        m.insert(0x11B6, [0x11AF, 0x11C2]); // ㅀ → ㄹ,ㅎ
        m.insert(0x11B9, [0x11B8, 0x11BA]); // ㅄ → ㅂ,ㅅ
        m
    });

// Compatibility jamo map: codepoint → compatibility codepoint
static COMPAT_JAMO: once_cell::sync::Lazy<std::collections::HashMap<u32, u32>> =
    once_cell::sync::Lazy::new(|| {
        let mut m = std::collections::HashMap::new();
        // choseong
        m.insert(0x1100, 0x3131); m.insert(0x1101, 0x3132);
        m.insert(0x1102, 0x3134); m.insert(0x1103, 0x3137);
        m.insert(0x1105, 0x3139); m.insert(0x1106, 0x3141);
        m.insert(0x1107, 0x3142); m.insert(0x1109, 0x3145);
        m.insert(0x110B, 0x3147); m.insert(0x110C, 0x3148);
        m.insert(0x110E, 0x314A); m.insert(0x110F, 0x314B);
        m.insert(0x1110, 0x314C); m.insert(0x1111, 0x314D);
        m.insert(0x1112, 0x314E);
        m.insert(0x1104, 0x3138); // ㄸ
        m.insert(0x1108, 0x3143); // ㅃ
        m.insert(0x110A, 0x3146); // ㅆ
        m.insert(0x110D, 0x3149); // ㅉ
        // jungseong
        m.insert(0x1161, 0x314F); m.insert(0x1162, 0x3150);
        m.insert(0x1165, 0x3153); m.insert(0x1166, 0x3154);
        m.insert(0x1167, 0x3155); m.insert(0x1168, 0x3156);
        m.insert(0x1169, 0x3157); m.insert(0x116E, 0x315C);
        m.insert(0x1172, 0x3161); m.insert(0x1173, 0x3163);
        m.insert(0x1175, 0x3163);
        // jongseong
        m.insert(0x11A8, 0x3131); m.insert(0x11AB, 0x3134);
        m.insert(0x11AF, 0x3139); m.insert(0x11B7, 0x3141);
        m.insert(0x11B8, 0x3142); m.insert(0x11BA, 0x3145);
        m.insert(0x11BC, 0x3147); m.insert(0x11BD, 0x3148);
        m.insert(0x11C2, 0x314E);
        m
    });

fn main() {
    // Register SHIFT+Backspace as a global hotkey (ID = 1)
    unsafe {
        let result = RegisterHotKey(ptr::null_mut(), 1, MOD_SHIFT, VK_BACK as u32);
        if result == 0 {
            println!("Failed to register hotkey!");
            return;
        }
        println!("Hotkey registered successfully. Press SHIFT+Backspace to trigger.");
    }

    // Message loop
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    unsafe {
        loop {
            let result = GetMessageW(&mut msg, ptr::null_mut(), 0, 0);
            if result == 0 || result == -1 {
                break;
            }
            
            if msg.message == WM_HOTKEY && msg.wParam == 1 {
                println!("Hotkey triggered!");
                handle_hotkey();
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn handle_hotkey() {
    // Get the currently focused window (for potential future use)
    let _hwnd = unsafe { GetForegroundWindow() };
    
    // 1) Save existing clipboard text with retry mechanism
    let old_clip = get_clipboard_with_retry().unwrap_or_default();
    // println!("Saved clipboard: {:?}", old_clip);


    // 2) Clear clipboard and wait
    clear_clipboard();
    thread::sleep(Duration::from_millis(50));

    
    // 3) Select previous character: SHIFT + ←
    println!("Selecting previous character...");
    send_key_combination(&[
        (VK_SHIFT, true),
        (VK_LEFT, true),
        (VK_LEFT, false),
        (VK_SHIFT, false),
    ]);
    thread::sleep(Duration::from_millis(50)); // Longer delay

    // 4) Copy with Ctrl+C
    println!("Copying selection...");
    send_key_combination(&[
        (VK_CONTROL, true),
        (0x43, true),  // 'C'
        (0x43, false),
        (VK_CONTROL, false),
    ]);
    thread::sleep(Duration::from_millis(50)); // Even longer delay for copy

    // 5) Read clipboard with retry
    let sel = get_clipboard_with_retry().unwrap_or_default();
    println!("Clipboard captured: {:?}", sel);
    
    // Restore old clipboard immediately to avoid interference
    set_clipboard(&old_clip);

    // 6) If not exactly one char, just send Backspace
    let chars: Vec<char> = sel.chars().collect();
    if chars.len() != 1 {
        println!("No single character selected, sending regular backspace");
        send_vk(VK_BACK, true);
        send_vk(VK_BACK, false);
        return;
    }
    let ch = chars[0];
    println!("Processing character: {:?} (U+{:04X})", ch, ch as u32);

    // 7) Run jamo-deletion logic
    let out = delete_one_jamo(ch);
    println!("Resulting jamos to type: {:?}", out);
    
    // 8) Delete original char
    send_vk(VK_BACK, true);
    send_vk(VK_BACK, false);
    thread::sleep(Duration::from_millis(50));

    // 9) Type result via keystroke map with proper handling for double consonants
    for j in out.chars() {
        if is_double_consonant(j) {
            // Handle double consonants with Shift + base key
            if let Some(vk) = jamo_to_double_keys(j) {
                println!("Typing double consonant: {:?} with Shift+{:02X}", j, vk);
                send_key_combination(&[
                    (VK_SHIFT, true),
                    (vk, true),
                    (vk, false),
                    (VK_SHIFT, false),
                ]);
                thread::sleep(Duration::from_millis(30));
            } else {
                println!("No double consonant mapping for: {:?}", j);
            }
        } else if let Some(keys) = jamo_to_keys(j) {
            // Handle regular jamos
            println!("Typing regular jamo: {:?}", j);
            for vk in keys {
                send_vk(vk, true);
                send_vk(vk, false);
                thread::sleep(Duration::from_millis(15));
            }
            thread::sleep(Duration::from_millis(30));
        } else {
            println!("No VK mapping for: {:?}", j);
        }
    }
}

fn send_key_combination(keys: &[(u16, bool)]) {
    for &(vk, down) in keys {
        send_vk(vk, down);
        thread::sleep(Duration::from_millis(10));
    }
}

fn get_clipboard_with_retry() -> Option<String> {
    for _attempt in 0..5 {
        if let Some(text) = get_clipboard() {
            return Some(text);
        }
        thread::sleep(Duration::from_millis(50));
    }
    None
}

/// Perform exactly one jamo deletion on a single Hangul syllable or jaso char,
/// returning a string of compatibility‐jamo characters (e.g. "읽"→"ㅇㅣㄹ").
fn delete_one_jamo(ch: char) -> String {
    // Check if it's already a compatibility jamo
    if ch as u32 >= 0x3131 && ch as u32 <= 0x318E {
        // It's already a compatibility jamo, just return empty (delete it)
        return String::new();
    }
    
    // Decompose NFD
    let dib: Vec<char> = ch.nfd().collect();
    if dib.len() < 2 {
        return String::new();
    }
    let choseong = dib[0];
    let jungseong = dib[1];
    // raw jongseong codepoint if any
    let jong_raw = dib.get(2).copied();
    
    // build jongseong list
    let mut jong: Vec<char> = if let Some(j) = jong_raw {
        let cp = j as u32;
        if let Some(split) = JONGSEONG_SPLIT.get(&cp) {
            split.iter().map(|&u| std::char::from_u32(u).unwrap()).collect()
        } else {
            vec![j]
        }
    } else {
        vec![]
    };

    // delete last jamo unit
    if !jong.is_empty() {
        jong.pop();
    } else {
        // drop jungseong, return only choseong
        return std::char::from_u32(COMPAT_JAMO.get(&(choseong as u32)).copied().unwrap_or(choseong as u32))
            .unwrap()
            .to_string();
    }

    // reassemble: [choseong, jungseong] + remaining jong
    let mut part = String::new();
    part.push(choseong);
    part.push(jungseong);
    for j in jong {
        part.push(j);
    }
    
    // Convert decomposed jamos directly to compatibility jamo
    part.chars()
        .filter_map(|c| {
            let cp = c as u32;
            COMPAT_JAMO.get(&cp).copied().and_then(std::char::from_u32)
        })
        .collect()
}

/// Simulate a single virtual-key event (down or up).
fn send_vk(vk: u16, down: bool) {
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if down { 0 } else { KEYEVENTF_KEYUP },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };

    unsafe {
        SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
    }
}

/// Read the current clipboard (unicode text).
fn get_clipboard() -> Option<String> {
    unsafe {
        // Try to open the clipboard with retry
        for _attempt in 0..3 {
            if OpenClipboard(ptr::null_mut()) != 0 {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        
        // Check if we successfully opened it
        if OpenClipboard(ptr::null_mut()) == 0 {
            return None;
        }
        
        let hmem = GetClipboardData(CF_UNICODETEXT as u32);
        if hmem.is_null() {
            CloseClipboard();
            return None;
        }
        
        let ptr = GlobalLock(hmem) as *const u16;
        if ptr.is_null() {
            CloseClipboard();
            return None;
        }
        
        // find the length up to the null terminator
        let mut len = 0;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        
        // build a slice and convert
        let slice = std::slice::from_raw_parts(ptr, len);
        let text = String::from_utf16_lossy(slice);
        
        GlobalUnlock(hmem);
        CloseClipboard();
        Some(text)
    }
}

/// Clear and set new clipboard text.
fn set_clipboard(s: &str) {
    unsafe {
        // Try to open clipboard with retry
        for _attempt in 0..3 {
            if OpenClipboard(ptr::null_mut()) != 0 {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        
        if OpenClipboard(ptr::null_mut()) == 0 { 
            return; 
        }
        
        EmptyClipboard();
        
        // allocate global and copy UTF-16
        let wide: Vec<u16> = s.encode_utf16().chain(Some(0)).collect();
        let size_bytes = wide.len() * std::mem::size_of::<u16>();
        
        // allocate movable global memory
        let h = GlobalAlloc(GHND, size_bytes);
        if !h.is_null() {
            let dst = GlobalLock(h) as *mut u16;
            if !dst.is_null() {
                ptr::copy_nonoverlapping(wide.as_ptr(), dst, wide.len());
                GlobalUnlock(h);
                SetClipboardData(CF_UNICODETEXT as u32, h);
            }
        }
        CloseClipboard();
    }
}

/// Empty the clipboard.
fn clear_clipboard() {
    unsafe {
        for _attempt in 0..3 {
            if OpenClipboard(ptr::null_mut()) != 0 {
                EmptyClipboard();
                CloseClipboard();
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}