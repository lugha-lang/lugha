/* Links for the confinement case, made the way Windows makes them, since
 * a Lugha program can't start `mklink` there yet. */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <winioctl.h>
#include <stdint.h>
#include <string.h>

/* UTF-8 `text` as UTF-16 in `out` (`cap` units). */
static void wide(const char *text, wchar_t *out, int cap) {
    if (MultiByteToWideChar(CP_UTF8, 0, text, -1, out, cap) == 0) {
        out[0] = 0;
    }
}

/* A symbolic link `name` to `target` (a folder when `folder`); 1 made. */
int64_t make_link(const char *target, const char *name, int64_t folder) {
    wchar_t t[1024], n[1024];
    wide(target, t, 1024);
    wide(name, n, 1024);
    DWORD flags = SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE |
                  (folder ? SYMBOLIC_LINK_FLAG_DIRECTORY : 0);
    return CreateSymbolicLinkW(n, t, flags) ? 1 : 0;
}

/* A junction `name` to the folder `target`, its full path taken here as
 * junctions need; 1 made. */
int64_t make_junction(const char *target, const char *name) {
    wchar_t t[1024], full[1024], n[1024];
    wide(target, t, 1024);
    wide(name, n, 1024);
    DWORD len = GetFullPathNameW(t, 1024, full, NULL);
    if (len == 0 || len > 900 || !CreateDirectoryW(n, NULL)) {
        return 0;
    }
    HANDLE h = CreateFileW(n, GENERIC_WRITE, 0, NULL, OPEN_EXISTING,
                           FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS, NULL);
    if (h == INVALID_HANDLE_VALUE) {
        return 0;
    }
    /* The mount point's data: the NT path (`\??\C:\…`), then the shown one. */
    static union {
        struct {
            DWORD tag;
            WORD data_len, reserved, sub_off, sub_len, print_off, print_len;
            WCHAR path[2048];
        } r;
        char bytes[16384];
    } buf;
    memset(&buf, 0, sizeof buf);
    WCHAR *p = buf.r.path;
    wcscpy(p, L"\\??\\");
    wcscat(p, full);
    WORD sub = (WORD)(wcslen(p) * 2);
    wcscpy(p + sub / 2 + 1, full);
    WORD print = (WORD)(wcslen(full) * 2);
    buf.r.tag = IO_REPARSE_TAG_MOUNT_POINT;
    buf.r.sub_off = 0;
    buf.r.sub_len = sub;
    buf.r.print_off = sub + 2;
    buf.r.print_len = print;
    buf.r.data_len = (WORD)(8 + sub + 2 + print + 2);
    DWORD got;
    BOOL ok = DeviceIoControl(h, FSCTL_SET_REPARSE_POINT, &buf, 8 + buf.r.data_len, NULL, 0,
                              &got, NULL);
    CloseHandle(h);
    return ok ? 1 : 0;
}
