//! Unix-only ownership regression: inherited flock references survive parent close.
use super::*;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

struct ForkChild(libc::pid_t);
impl Drop for ForkChild {
    fn drop(&mut self) {
        if self.0 > 0 {
            // Also reap on assertion failure; never leave inherited leases alive.
            unsafe {
                libc::kill(self.0, libc::SIGKILL);
                libc::waitpid(self.0, std::ptr::null_mut(), 0);
            }
        }
    }
}
fn readable(fd: &OwnedFd) {
    let mut poll = libc::pollfd {
        fd: fd.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    assert_eq!(
        unsafe { libc::poll(&mut poll, 1, 5000) },
        1,
        "child handshake timed out"
    );
    assert_eq!(poll.revents & (libc::POLLERR | libc::POLLNVAL), 0);
}

#[test]
fn owner_drop_releases_lease_while_child_still_holds_inherited_descriptor() {
    let root = fixture();
    let control_root = fixture();
    let control = OpenOptions::new()
        .read(true)
        .write(true)
        .open(control_root.path().join("lease"))
        .unwrap();
    FileExt::try_lock_exclusive(&control).unwrap();
    let lease = Lease::open(root.path()).unwrap();
    let mut fds = [-1; 2];
    assert_eq!(
        unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, fds.as_mut_ptr()) },
        0
    );
    // SAFETY: successful socketpair returns two owned descriptors.
    let parent = unsafe { OwnedFd::from_raw_fd(fds[0]) };
    let child_socket = unsafe { OwnedFd::from_raw_fd(fds[1]) };
    // Unrelated parallel exec children must not retain the handshake sockets.
    // Our controlled fork child intentionally retains them until _exit.
    for fd in fds {
        assert_eq!(
            unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) },
            0
        );
    }
    // SAFETY: after fork the child calls ONLY async-signal-safe libc operations,
    // uses stack buffers, and _exit; no Rust allocation, locks or destructors.
    let pid = unsafe { libc::fork() };
    assert!(pid >= 0, "fork failed");
    if pid == 0 {
        unsafe {
            libc::close(fds[0]);
            let ready = [1u8];
            if libc::write(fds[1], ready.as_ptr().cast(), 1) != 1 {
                libc::_exit(2);
            }
            let mut command = [0u8];
            if libc::read(fds[1], command.as_mut_ptr().cast(), 1) != 1 {
                libc::_exit(3);
            }
            libc::_exit(0);
        }
    }
    let mut child = ForkChild(pid);
    drop(child_socket);
    readable(&parent);
    let mut ready = [0u8];
    assert_eq!(
        unsafe { libc::read(parent.as_raw_fd(), ready.as_mut_ptr().cast(), 1) },
        1
    );
    assert_eq!(ready, [1]);

    drop(control); // Negative control: File::drop alone is not the final close.
    let contender = OpenOptions::new()
        .read(true)
        .write(true)
        .open(control_root.path().join("lease"))
        .unwrap();
    assert_eq!(
        FileExt::try_lock_exclusive(&contender).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    assert!(Lease::open(root.path()).is_err()); // Owner is still live.
    drop(lease);
    // No sleep, retry, or child exit between release and this ONE acquisition.
    let next = Lease::open(root.path()).unwrap();
    assert!(Lease::open(root.path()).is_err()); // New owner remains exclusive.
    drop(next);
    let _again = Lease::open(root.path()).unwrap();
    assert_eq!(
        unsafe { libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG) },
        0
    );

    let exit = [0u8];
    assert_eq!(
        unsafe { libc::write(parent.as_raw_fd(), exit.as_ptr().cast(), 1) },
        1
    );
    readable(&parent);
    assert_eq!(
        unsafe { libc::read(parent.as_raw_fd(), ready.as_mut_ptr().cast(), 1) },
        0
    );
    let mut status = 0;
    assert_eq!(unsafe { libc::waitpid(pid, &mut status, 0) }, pid);
    child.0 = 0;
    assert!(libc::WIFEXITED(status));
    assert_eq!(libc::WEXITSTATUS(status), 0);
    // Positive control: the retained child reference was the only remaining lock owner.
    FileExt::try_lock_exclusive(&contender).unwrap();
}
