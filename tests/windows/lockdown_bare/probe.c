/* What C code in a locked-down program can do: start a program, write a
 * file, read one. */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdint.h>

/* `cmd.exe /c exit 7` started and waited for: its exit code, or -1 when
 * Windows refuses to start it. */
int64_t start_one(void) {
    wchar_t line[] = L"cmd.exe /c exit 7";
    STARTUPINFOW si;
    PROCESS_INFORMATION pi;
    ZeroMemory(&si, sizeof si);
    si.cb = sizeof si;
    if (!CreateProcessW(NULL, line, NULL, NULL, FALSE, CREATE_NO_WINDOW, NULL, NULL, &si,
                        &pi)) {
        return -1;
    }
    DWORD code = 0;
    WaitForSingleObject(pi.hProcess, INFINITE);
    if (!GetExitCodeProcess(pi.hProcess, &code)) {
        code = (DWORD)-2;
    }
    CloseHandle(pi.hThread);
    CloseHandle(pi.hProcess);
    return (int64_t)(int32_t)code;
}

/* 1 when a new file can be made in the working folder (then removed). */
int64_t can_write(void) {
    HANDLE h = CreateFileW(L"probe_made.txt", GENERIC_WRITE, 0, NULL, CREATE_ALWAYS,
                           FILE_ATTRIBUTE_NORMAL, NULL);
    if (h == INVALID_HANDLE_VALUE) {
        return 0;
    }
    CloseHandle(h);
    DeleteFileW(L"probe_made.txt");
    return 1;
}

/* 1 when this program's own file can be opened and read. */
int64_t can_read(void) {
    wchar_t self[MAX_PATH];
    DWORD n = GetModuleFileNameW(NULL, self, MAX_PATH);
    if (n == 0 || n == MAX_PATH) {
        return 0;
    }
    HANDLE h = CreateFileW(self, GENERIC_READ, FILE_SHARE_READ, NULL, OPEN_EXISTING,
                           FILE_ATTRIBUTE_NORMAL, NULL);
    if (h == INVALID_HANDLE_VALUE) {
        return 0;
    }
    char head[2];
    DWORD got = 0;
    BOOL read = ReadFile(h, head, 2, &got, NULL);
    CloseHandle(h);
    return read && got == 2 && head[0] == 'M' && head[1] == 'Z';
}
