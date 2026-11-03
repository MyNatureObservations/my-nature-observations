//! Windows-only ACL operations. No inherited broad permissions are accepted.
use super::*;
use std::{
    ffi::c_void,
    mem::{size_of, zeroed},
    os::windows::{ffi::OsStrExt, io::AsRawHandle},
    ptr,
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, LocalFree},
    Security::{Authorization::*, *},
    Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, CreateDirectoryW, FILE_ALL_ACCESS, GetFileInformationByHandle,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};
struct Allocation(*mut c_void);
impl Drop for Allocation {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
struct Token(HANDLE);
impl Drop for Token {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}
fn fail() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "private Windows ACL required",
    )
}
fn user_sid() -> io::Result<String> {
    unsafe {
        let mut token = ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(io::Error::last_os_error());
        }
        let token = Token(token);
        let mut length = 0;
        GetTokenInformation(token.0, TokenUser, ptr::null_mut(), 0, &mut length);
        if length == 0 || length > 64 * 1024 {
            return Err(fail());
        }
        let mut buffer = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
        if GetTokenInformation(
            token.0,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            length,
            &mut length,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let user = &*buffer.as_ptr().cast::<TOKEN_USER>();
        let mut text = ptr::null_mut();
        if ConvertSidToStringSidW(user.User.Sid, &mut text) == 0 {
            return Err(io::Error::last_os_error());
        }
        let _text = Allocation(text.cast());
        let mut len = 0;
        while *text.add(len) != 0 {
            len += 1;
        }
        String::from_utf16(std::slice::from_raw_parts(text, len)).map_err(|_| fail())
    }
}
fn descriptor() -> io::Result<Allocation> {
    let user = user_sid()?;
    // Protected DACL: current user + SYSTEM only, inherited by files/subdirectories.
    let text: Vec<u16> = format!("D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;{user})")
        .encode_utf16()
        .chain(Some(0))
        .collect();
    unsafe {
        let mut sd = ptr::null_mut();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            text.as_ptr(),
            1,
            &mut sd,
            ptr::null_mut(),
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Allocation(sd))
    }
}
pub(super) fn create_dir(path: &Path) -> io::Result<()> {
    let sd = descriptor()?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd.0,
        bInheritHandle: 0,
    };
    unsafe {
        if CreateDirectoryW(wide(path).as_ptr(), &attributes) == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    if let Err(e) = verify_private(path, true) {
        let _ = fs::remove_dir(path);
        return Err(e);
    }
    Ok(())
}
unsafe fn dacl(sd: PSECURITY_DESCRIPTOR) -> io::Result<*mut ACL> {
    unsafe {
        let mut acl = ptr::null_mut();
        let mut present = 0;
        let mut defaulted = 0;
        if GetSecurityDescriptorDacl(sd, &mut present, &mut acl, &mut defaulted) == 0
            || present == 0
            || acl.is_null()
        {
            return Err(fail());
        }
        Ok(acl)
    }
}
unsafe fn ace(acl: *mut ACL, index: u32) -> io::Result<*const ACCESS_ALLOWED_ACE> {
    unsafe {
        let mut entry = ptr::null_mut();
        if GetAce(acl, index, &mut entry) == 0 {
            return Err(fail());
        }
        let entry = entry.cast::<ACCESS_ALLOWED_ACE>();
        if (*entry).Header.AceType != 0
            || u32::from((*entry).Header.AceFlags) & INHERIT_ONLY_ACE != 0
            || (*entry).Mask != FILE_ALL_ACCESS
        {
            return Err(fail());
        }
        Ok(entry)
    }
}
pub(super) fn verify_private(path: &Path, directory: bool) -> io::Result<()> {
    let expected = descriptor()?;
    unsafe {
        let mut sd = ptr::null_mut();
        if GetNamedSecurityInfoW(
            wide(path).as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            &mut sd,
        ) != 0
        {
            return Err(fail());
        }
        let actual = Allocation(sd);
        let actual_acl = dacl(actual.0)?;
        let expected_acl = dacl(expected.0)?;
        let mut control = 0;
        let mut revision = 0;
        if GetSecurityDescriptorControl(actual.0, &mut control, &mut revision) == 0
            || (directory && control & SE_DACL_PROTECTED == 0)
        {
            return Err(fail());
        }
        if (*actual_acl).AceCount != 2 || (*expected_acl).AceCount != 2 {
            return Err(fail());
        }
        let mut found = [false; 2];
        for i in 0..2 {
            let a = ace(actual_acl, i)?;
            for j in 0..2 {
                let e = ace(expected_acl, j)?;
                if EqualSid(
                    ptr::addr_of!((*a).SidStart).cast_mut().cast(),
                    ptr::addr_of!((*e).SidStart).cast_mut().cast(),
                ) != 0
                {
                    found[j as usize] = true;
                }
            }
        }
        if !found.into_iter().all(|v| v) {
            return Err(fail());
        }
        Ok(())
    }
}
pub(super) fn identity(file: &File) -> io::Result<(u32, u64)> {
    unsafe {
        let mut info: BY_HANDLE_FILE_INFORMATION = zeroed();
        if GetFileInformationByHandle(file.as_raw_handle(), &mut info) == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok((
            info.dwVolumeSerialNumber,
            (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
        ))
    }
}
