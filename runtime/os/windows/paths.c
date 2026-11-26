/*
 * os/windows/paths.c — part of lugha_rt.c on Windows: what a confined
 * lookup (dir_resolve.c) and a `Dir`'s operations need below the walk:
 * which names and paths Windows can't be given, one name opened from a
 * folder handle as itself, a handle opened again, and Windows' failures
 * as `errno` and in Linux's words.
 *
 * A fragment, not compiled on its own: joined before dir_resolve.c.
 */

#include <winternl.h>
#include <winioctl.h>

/* Links followed in one lookup (Linux's limit); a path's length with its
 * links expanded; one name's length in bytes. */
enum { WALK_LINKS = 40, WALK_MAX = 4096, WALK_NAME = 1024 };

/* Failures of Windows' own, beside `errno`'s: a name Windows reserves, a
 * reparse point that isn't a link, a name that isn't valid Unicode, and
 * two volumes (`EXDEV` means "outside" here). */
enum { WIN_RESERVED = 10001, WIN_ODD_REPARSE, WIN_NOT_UNICODE, WIN_CROSS };

/* A reparse point's data, as the file system gives it (the DDK's
 * `REPARSE_DATA_BUFFER`, symbolic links' form). */
typedef struct {
    ULONG tag;
    USHORT data_len;
    USHORT reserved;
    USHORT sub_offset;
    USHORT sub_len;
    USHORT print_offset;
    USHORT print_len;
    ULONG flags;
    WCHAR path[1];
} WinLink;

/* A symbolic link whose target is relative to its folder. */
#define WIN_LINK_RELATIVE 1

/* What a lookup is for: the access wanted on the last name, whether it
 * must be a file (1) or a folder (2) or either (0), and whether a missing
 * file is made (`create`). */
typedef struct {
    ACCESS_MASK access;
    int kind;
    bool create;
} WinWant;

/* `errno` for an NTSTATUS a lookup or change failed with. */
static int win_errno_nt(NTSTATUS s) {
    switch ((ULONG)s) {
    case 0xC0000034: // STATUS_OBJECT_NAME_NOT_FOUND
    case 0xC000003A: // STATUS_OBJECT_PATH_NOT_FOUND
    case 0xC0000056: // STATUS_DELETE_PENDING
        return ENOENT;
    case 0xC0000035: // STATUS_OBJECT_NAME_COLLISION
        return EEXIST;
    case 0xC0000022: // STATUS_ACCESS_DENIED
    case 0xC0000121: // STATUS_CANNOT_DELETE
        return EACCES;
    case 0xC0000043: // STATUS_SHARING_VIOLATION
        return EBUSY;
    case 0xC00000BA: // STATUS_FILE_IS_A_DIRECTORY
        return EISDIR;
    case 0xC0000103: // STATUS_NOT_A_DIRECTORY
        return ENOTDIR;
    case 0xC0000101: // STATUS_DIRECTORY_NOT_EMPTY
        return ENOTEMPTY;
    case 0xC00000D4: // STATUS_NOT_SAME_DEVICE
        return WIN_CROSS;
    case 0xC0000106: // STATUS_NAME_TOO_LONG
        return ENAMETOOLONG;
    case 0xC000007F: // STATUS_DISK_FULL
        return ENOSPC;
    case 0xC0000033: // STATUS_OBJECT_NAME_INVALID
        return EINVAL;
    default:
        return EIO;
    }
}

/* `errno` for a Win32 error from the last call that failed. */
static int win_errno_last(void) {
    switch (GetLastError()) {
    case ERROR_FILE_NOT_FOUND:
    case ERROR_PATH_NOT_FOUND:
        return ENOENT;
    case ERROR_FILE_EXISTS:
    case ERROR_ALREADY_EXISTS:
        return EEXIST;
    case ERROR_ACCESS_DENIED:
        return EACCES;
    case ERROR_SHARING_VIOLATION:
        return EBUSY;
    case ERROR_DIR_NOT_EMPTY:
        return ENOTEMPTY;
    case ERROR_DIRECTORY:
        return ENOTDIR;
    case ERROR_NOT_SAME_DEVICE:
        return WIN_CROSS;
    case ERROR_DISK_FULL:
        return ENOSPC;
    case ERROR_INVALID_PARAMETER:
    case ERROR_INVALID_NAME:
        return EINVAL;
    default:
        return EIO;
    }
}

/* Linux's words for each failure, so a program reads the same message on
 * every system. */
static const char *win_message(int code) {
    switch (code) {
    case ENOENT:
        return "No such file or directory";
    case EEXIST:
        return "File exists";
    case EACCES:
        return "Permission denied";
    case EBUSY:
        return "Device or resource busy";
    case EISDIR:
        return "Is a directory";
    case ENOTDIR:
        return "Not a directory";
    case ENOTEMPTY:
        return "Directory not empty";
    case ENAMETOOLONG:
        return "File name too long";
    case ENOSPC:
        return "No space left on device";
    case EINVAL:
        return "Invalid argument";
    case ELOOP:
        return "Too many levels of symbolic links";
    case EMFILE:
        return "Too many open files";
    case WIN_CROSS:
        return "Invalid cross-device link";
    case WIN_RESERVED:
        return "path holds a name Windows reserves";
    case WIN_ODD_REPARSE:
        return "can't follow a reparse point that isn't a link";
    case WIN_NOT_UNICODE:
        return "a name is not valid Unicode";
    default:
        return "Input/output error";
    }
}

/* `n` UTF-8 bytes as NUL-ended UTF-16 in `out` (`cap` units): its length,
 * or -1 when it isn't UTF-8 or doesn't fit. */
static int win_wide(const char *text, size_t n, wchar_t *out, int cap) {
    if (n == 0) {
        out[0] = 0;
        return 0;
    }
    int got = n > INT_MAX ? 0 : MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, text, (int)n,
                                                    out, cap - 1);
    if (got <= 0) {
        return -1;
    }
    out[got] = 0;
    return got;
}

/* Whether the name's base (before its first `.`, trailing spaces and dots
 * dropped) is a device name, in any case: `CON`, `PRN`, `AUX`, `NUL`,
 * `CONIN$`, `CONOUT$`, `COM` or `LPT` and a digit 1–9 or a superscript ¹²³. */
static bool win_device(const char *name, size_t n) {
    size_t base = 0;
    while (base < n && name[base] != '.') {
        base++;
    }
    while (base > 0 && (name[base - 1] == ' ' || name[base - 1] == '.')) {
        base--;
    }
    char up[8];
    if (base > 7) {
        return false;
    }
    for (size_t i = 0; i < base; i++) {
        char c = name[i];
        up[i] = (c >= 'a' && c <= 'z') ? (char)(c - 32) : c;
    }
    up[base] = '\0';
    static const char *const fixed[] = {"CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"};
    for (size_t i = 0; i < sizeof fixed / sizeof fixed[0]; i++) {
        if (strcmp(up, fixed[i]) == 0) {
            return true;
        }
    }
    bool port = strncmp(up, "COM", 3) == 0 || strncmp(up, "LPT", 3) == 0;
    if (!port) {
        return false;
    }
    if (base == 4) {
        return up[3] >= '1' && up[3] <= '9';
    }
    // ¹ ² ³ in UTF-8: C2 B9, C2 B2, C2 B3.
    return base == 5 && (unsigned char)up[3] == 0xC2 &&
           ((unsigned char)up[4] == 0xB9 || (unsigned char)up[4] == 0xB2 ||
            (unsigned char)up[4] == 0xB3);
}

/* Whether `name` (`n` bytes, not `.` or `..`) may name something in a
 * folder on Windows. */
static bool win_name_ok(const char *name, size_t n) {
    for (size_t i = 0; i < n; i++) {
        unsigned char c = (unsigned char)name[i];
        if (c < 0x20 || c == '*' || c == '"' || c == '<' || c == '>' || c == '|') {
            return false;
        }
    }
    if (n > 0 && (name[n - 1] == '.' || name[n - 1] == ' ')) {
        return false;
    }
    return !win_device(name, n);
}

/* Whether `path` leads outside before any name is looked at: it starts at
 * a root, or holds `:` or `?`. */
static bool win_outside(const char *path, size_t n) {
    if (n > 0 && (path[0] == '/' || path[0] == '\\')) {
        return true;
    }
    return memchr(path, ':', n) != NULL || memchr(path, '?', n) != NULL;
}

/* One name (`n` UTF-8 bytes) opened from the folder `at` as itself:
 * `access`, `disposition` and `options` as `NtCreateFile` takes them;
 * NTSTATUS, and the handle in `*out`. */
static NTSTATUS win_open_name(HANDLE at, const char *name, size_t n, ACCESS_MASK access,
                              ULONG disposition, ULONG options, HANDLE *out) {
    wchar_t wide[WALK_NAME];
    int len = win_wide(name, n, wide, WALK_NAME);
    if (len < 0) {
        return (NTSTATUS)0xC0000033; // STATUS_OBJECT_NAME_INVALID
    }
    UNICODE_STRING text = {(USHORT)(len * 2), (USHORT)(len * 2), wide};
    OBJECT_ATTRIBUTES attrs;
    InitializeObjectAttributes(&attrs, &text, OBJ_CASE_INSENSITIVE, at, NULL);
    IO_STATUS_BLOCK io;
    return NtCreateFile(out, access | SYNCHRONIZE, &attrs, &io, NULL, FILE_ATTRIBUTE_NORMAL,
                        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, disposition,
                        options | FILE_OPEN_REPARSE_POINT | FILE_OPEN_FOR_BACKUP_INTENT |
                            FILE_SYNCHRONOUS_IO_NONALERT,
                        NULL, 0);
}

/* `h` opened again with `access`: the same file, whatever its name is now;
 * NULL with `errno` set. An empty name relative to `h`, as `ReOpenFile`
 * does, but without the `FILE_NON_DIRECTORY_FILE` it adds, which refuses
 * every folder. */
static HANDLE win_reopen(HANDLE h, ACCESS_MASK access, bool folder) {
    HANDLE again;
    NTSTATUS s = win_open_name(h, "", 0, access, FILE_OPEN,
                               folder ? FILE_DIRECTORY_FILE : FILE_NON_DIRECTORY_FILE, &again);
    if (!NT_SUCCESS(s)) {
        errno = win_errno_nt(s);
        return NULL;
    }
    return again;
}

/* What `h` is: its attributes and reparse tag; false with `errno` set. */
static bool win_tag(HANDLE h, FILE_ATTRIBUTE_TAG_INFO *tag) {
    if (!GetFileInformationByHandleEx(h, FileAttributeTagInfo, tag, sizeof *tag)) {
        errno = win_errno_last();
        return false;
    }
    return true;
}

/* Information set on `h` through the NT call itself (`class` as
 * `NtSetInformationFile` numbers them): 0, or the failure's `errno`;
 * `*unknown` when the class or its flags aren't known to this file
 * system or Windows, so an older form may be tried. Kernel32's wrapper
 * isn't used: some Windows-likes refuse a rename relative to a folder
 * handle through it, which the NT call takes. */
static int win_set_info(HANDLE h, ULONG class, void *data, ULONG size, bool *unknown) {
    IO_STATUS_BLOCK io;
    NTSTATUS s = NtSetInformationFile(h, &io, data, size, (FILE_INFORMATION_CLASS)class);
    ULONG code = (ULONG)s;
    *unknown = code == 0xC0000003 || code == 0xC00000BB || code == 0xC000000D;
    return NT_SUCCESS(s) ? 0 : win_errno_nt(s);
}
