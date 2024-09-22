use std::ffi::{c_uint, c_void};

use anyhow::{bail, Result};
use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::{
            GetLastError, SetLastError, HINSTANCE, HMODULE, HWND, LPARAM, LRESULT, RECT,
            WIN32_ERROR, WPARAM,
        },
        Graphics::Gdi::{
            BeginPaint, EndPaint, FillRect, UpdateWindow, COLOR_WINDOW, HBRUSH, HDC, PAINTSTRUCT,
            SYS_COLOR_INDEX,
        },
        System::SystemServices::IMAGE_DOS_HEADER,
        UI::WindowsAndMessaging::*,
    },
};

use crate::utils::strings::str_to_wstr;

pub struct WindowsWindow {}

impl WindowsWindow {
    pub fn new(title: &str, width: Option<i32>, height: Option<i32>) -> Result<Self> {
        // Get Application Instance Handle
        let h_instance = get_instance_handle();

        let window_class = w!("window");

        let window_title = PCWSTR(str_to_wstr(title).as_ptr());

        Self::register_class(h_instance, window_class)?;
        Self::init_instance(
            h_instance,
            window_class,
            window_title,
            SW_SHOW,
            width,
            height,
        );
        Ok(Self {})
    }

    fn register_class(h_instance: HMODULE, class_name: PCWSTR) -> Result<()> {
        // Crete empty WNDCLASSW (Wide)
        let mut wc = WNDCLASSW::default();

        // Fill minimum requirements
        //wc.style = CS_HREDRAW | CS_VREDRAW;
        wc.lpfnWndProc = Some(Self::window_procedure);
        wc.hInstance = h_instance.into();
        wc.hCursor = load_default_cursor(IDC_ARROW)?;
        wc.lpszClassName = class_name;

        // Register Window Class (WNDCLASSW)
        let atom = unsafe { RegisterClassW(&wc) };
        if atom == 0 {
            let last_error = unsafe { GetLastError() };
            bail!(
                "Could not register the window class, error code: {:?}",
                last_error
            );
        }

        Ok(())
    }

    fn init_instance(
        h_instance: HMODULE,
        class_name: PCWSTR,
        window_title: PCWSTR,
        n_cmd_show: SHOW_WINDOW_CMD,
        width: Option<i32>,
        height: Option<i32>,
    ) {
        // Prepare app data
        let lparam: *mut i32 = Box::leak(Box::new(5_i32));

        // Create window of class wc and get Handle
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_RIGHTSCROLLBAR,
                class_name,
                window_title,
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                0,
                width.unwrap_or(CW_USEDEFAULT),
                height.unwrap_or(0),
                HWND::default(),
                HMENU::default(),
                h_instance,
                Some(lparam.cast()),
            )
            .unwrap()
        };

        // Show created window
        let code = unsafe { ShowWindow(hwnd, n_cmd_show) };
        if code.0 != 0 {
            let last_error = unsafe { GetLastError() };
            panic!("Could not create window, error code: {:?}", last_error);
        }
        unsafe {
            UpdateWindow(hwnd).unwrap();
        };
    }

    pub unsafe extern "system" fn window_procedure(
        hwnd: HWND,
        msg: c_uint,
        w_param: WPARAM,
        l_param: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_NCCREATE => {
                let createstruct: *mut CREATESTRUCTW = l_param.0 as *mut _;
                if createstruct.is_null() {
                    return LRESULT(0);
                }
                //Set Window Title
                SetWindowTextW(hwnd, (*createstruct).lpszName).unwrap();

                let ptr: *mut i32 = (*createstruct).lpCreateParams.cast();
                return LRESULT(set_window_userdata::<i32>(hwnd, ptr).is_ok() as isize);
            }
            //WM_CREATE => (),
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
            }
            WM_DESTROY => {
                match get_window_userdata::<i32>(hwnd) {
                    Ok(ptr) if !ptr.is_null() => {
                        let _ = Box::from_raw(ptr);
                    }
                    Ok(_) => (),
                    Err(e) => {
                        println!(
                            "Error while getting the userdata ptr to clean it up: {:?}",
                            e
                        );
                    }
                }
                post_quit_message(0);
            }
            WM_PAINT => {
                do_some_painting(hwnd, |hdc, _erase_bg, target_rect| {
                    let _ = fill_rect_with_sys_color(hdc, &target_rect, COLOR_WINDOW);
                    Ok(())
                })
                .unwrap_or_else(|e| println!("Error during painting: {:?}", e));
            }
            _ => return DefWindowProcW(hwnd, msg, w_param, l_param),
        }
        LRESULT(0)
    }

    pub fn window_loop(&self) {
        loop {
            match get_next_message() {
                Ok(msg) => {
                    if msg.message == WM_QUIT {
                        std::process::exit(msg.wParam.0 as i32);
                    }
                    let _ = translte_message(&msg);
                    unsafe {
                        DispatchMessageW(&msg);
                    }
                }
                Err(e) => panic!("Failed getting next message: {}", e),
            }
        }
    }
}

pub fn get_instance_handle() -> HMODULE {
    extern "C" {
        static __ImageBase: IMAGE_DOS_HEADER;
    }

    HMODULE(unsafe { &__ImageBase as *const _ as *mut c_void })
}

pub fn load_default_cursor(cursor: PCWSTR) -> Result<HCURSOR> {
    let hcursor = unsafe { LoadCursorW(HINSTANCE::default(), cursor).unwrap() };
    if hcursor.is_invalid() {
        bail!("Failed to load predefined cursor");
    } else {
        Ok(hcursor)
    }
}

pub fn get_next_message() -> Result<MSG> {
    let mut msg = MSG::default();
    let output = unsafe { GetMessageW(&mut msg, HWND::default(), 0, 0) };
    if output.0 >= 0 {
        Ok(msg)
    } else {
        bail!("Failed getting next message")
    }
}

pub fn translte_message(msg: &MSG) -> Result<bool> {
    let res = unsafe { TranslateMessage(msg) };
    match res.ok() {
        Ok(_) => Ok(0 != res.0),
        Err(err) => Err(err.into()),
    }
}

pub unsafe fn set_window_userdata<T>(hwnd: HWND, ptr: *mut T) -> Result<*mut T, WIN32_ERROR> {
    SetLastError(WIN32_ERROR(0));
    let out = SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize);
    if out == 0 {
        let last_error = GetLastError();
        if last_error.0 != 0 {
            Err(last_error)
        } else {
            Ok(out as *mut T)
        }
    } else {
        Ok(out as *mut T)
    }
}

pub unsafe fn get_window_userdata<T>(hwnd: HWND) -> Result<*mut T, WIN32_ERROR> {
    SetLastError(WIN32_ERROR(0));
    let out = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
    if out == 0 {
        let last_error = GetLastError();
        if last_error.0 != 0 {
            Err(last_error)
        } else {
            Ok(out as *mut T)
        }
    } else {
        Ok(out as *mut T)
    }
}

pub fn post_quit_message(exit_code: i32) {
    unsafe {
        PostQuitMessage(exit_code);
    }
}

pub unsafe fn begin_paint(hwnd: HWND) -> Result<(HDC, PAINTSTRUCT), WIN32_ERROR> {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    if hdc.is_invalid() {
        Err(GetLastError())
    } else {
        Ok((hdc, ps))
    }
}

pub unsafe fn fill_rect_with_sys_color(
    hdc: HDC,
    rect: &RECT,
    color: SYS_COLOR_INDEX,
) -> Result<(), ()> {
    if FillRect(hdc, rect, (HBRUSH)((color.0 + 1) as *mut c_void)) != 0 {
        Ok(())
    } else {
        Err(())
    }
}

pub unsafe fn end_paint(hwnd: HWND, ps: &PAINTSTRUCT) {
    EndPaint(hwnd, ps).unwrap();
}

pub unsafe fn do_some_painting<F, T>(hwnd: HWND, f: F) -> Result<T, WIN32_ERROR>
where
    F: FnOnce(HDC, bool, RECT) -> Result<T, WIN32_ERROR>,
{
    let (hdc, ps) = begin_paint(hwnd)?;
    let output = f(hdc, ps.fErase.as_bool(), ps.rcPaint);
    end_paint(hwnd, &ps);
    output
}
