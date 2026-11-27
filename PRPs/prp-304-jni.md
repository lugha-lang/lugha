## FEATURE: Kotlin and Java call a Lugha library, `lughac build --shared --jni` — milestone 286

## OBJECTIVE

An Android app's Kotlin or Java code (or any JVM program's) calls a Lugha library's exported functions (`extern fun … { … }`, milestone 284) as ordinary methods, and a Lugha panic reaches it as a Java exception.

```lugha
// words.la
extern fun word_count(text: string): i64 = text.split(" ").len;
extern fun pick(i: i64): i64 {
    let xs = [1, 2, 3];
    xs[i]
}
```
```sh
lughac build --shared --jni com.example.Words words.la \
    --target aarch64-linux-android -o app/src/main/jniLibs/arm64-v8a/libwords.so
# also writes app/src/main/jniLibs/arm64-v8a/com/example/Words.kt (moved into the app's source folder)
```
```kotlin
// com/example/Words.kt, as lughac writes it
package com.example

object Words {
    init { System.loadLibrary("words") }

    class Panic(message: String) : RuntimeException(message)

    @JvmStatic external fun wordCount(text: String): Long
    @JvmStatic external fun pick(i: Long): Long
}
```
```kotlin
val n = Words.wordCount("the quick brown fox")   // 4
try {
    Words.pick(7)
} catch (e: Words.Panic) {
    Log.e("words", e.message!!)   // "index out of bounds: the length is 3 but the index is 7"
}
```

This is stage 2 of Android (decision 229): stage 1 (command-line programs) is milestone 283, and the C side it stands on is milestone 284.

🔒 **Security-sensitive:** lughac generates C that it compiles into the library (`src/link/`'s C files), and a panic now has a second ending, through the runtime's unwinding (`runtime/tasks/`). This needs your review before any code.

## CONTEXT

- **Read first:** `src/driver/linking.rs` (`build_shared`), `src/driver/c_header.rs` (types per export), `src/link/inputs.rs` (`Plan::c_files`), `src/link/shared.rs` (what a library shows), `runtime/tasks/exports.c` (`lugha_rt_export_run`), `runtime/tasks/unwind.c` (`panic_stop`), `tests/exports.rs`.
- **Spec:** section 8's exported functions gain "called from Java"; section 9's `--shared` row gains `--jni`.
- **Decisions:** the user chose (2026-11-27) generated glue (as Go's `gomobile bind` and Mozilla's `uniffi` do) over hand-written JNI, a panic as a Java exception rather than the app ending, and Kotlin source by default with Java on request (`--jni-lang java`). Decision 229 (Android: command line, then libraries). Nothing open.

## RESEARCH (summary)

- **JNI** finds a native method by its C name, `Java_<package>_<Class>_<method>` (`_` in a name written `_1`, `.` as `_`), taking `JNIEnv *` and the class first. Numbers and `boolean` pass as C's types (`jint` is `int32_t`, `jlong` `int64_t`, `jboolean` `uint8_t`); a `String` is a `jstring` read through JNI's function table.
- **JNI's strings are "modified UTF-8"** (NUL as two bytes, characters outside the BMP as two 3-byte surrogates): `GetStringUTFChars` and `NewStringUTF` don't give or take real UTF-8. `GetStringChars`/`NewString` (UTF-16) do, converted by the glue.
- **JNI's function table** (`JNIEnv`'s functions, in order) is fixed by the JNI specification and never reordered: the glue declares the few entries it uses itself, so no JDK or NDK header is needed and any target's `--shared` build can carry it.
- **Go** (`gomobile bind`) and **Rust** (`uniffi`, `jni`) both turn a native panic into a Java exception at the boundary; an uncaught one crashes the app as any Java exception does.

## REQUIREMENTS

### Must Do

**The build**
- `lughac build --shared --jni <package.Class> lib.la` (`--jni` only with `--shared`, exit 2 otherwise): the library also holds a JNI function for each export, and the class's source is written beside it, in folders by package.
- **Kotlin by default:** `com/example/Words.kt`, an `object Words` with an `@JvmStatic external fun` per export, an `init` block loading the library (`System.loadLibrary("words")`, from the library's file name), and `class Panic(message: String) : RuntimeException(message)`; `Option<string>` is `String?`.
- **`--jni-lang java`** (only with `--jni`) writes `com/example/Words.java` instead: `public final class Words` with a `static native` method per export, a static block loading the library, and `public static final class Panic extends RuntimeException`.
- Either way the methods are named after the Lugha name in lowerCamelCase (`word_count` → `wordCount`), and the JNI names are the same (`Java_com_example_Words_wordCount`), so the glue doesn't depend on the language.
- The library shows the JNI functions (and, still, the C exports).

**Types** (an export Java can't call is E0487 under `--jni`, naming the parameter or result and what Java takes)
- `i8`, `i16`, `i32`, `i64`, `f32`, `f64`, `bool` → `byte`, `short`, `int`, `long`, `float`, `double`, `boolean`; `char` → `int` (the code point).
- `string` → `String`, converted between Java's UTF-16 and Lugha's UTF-8 exactly (no "modified UTF-8"; a lone surrogate becomes U+FFFD); `Option<string>` → a `String` that may be `null`; a `string` result → `String`. A Java string holding NUL is refused with `IllegalArgumentException` (an export's C text would end there), and a Lugha one returned to Java panics (`Words.Panic`), as for C callers.
- A handle and `Option` of one → `long` (the pointer; `0` for `None`).
- Unsigned integers (Java has none) and extern structs (no value type in Java) are E0487.

**Panics**
- A panic in an export called through the glue unwinds its task as usual (its `defer`s run), then the glue throws `Words.Panic` with the panic's message; the export's result is never read. A C caller, through the export itself, still ends the process with 101.

**Calls**
- As every export: the calling thread waits while the body runs as a task, so Java may call from any thread, several at once. A call from Android's main thread blocks it, as any blocking call does (the docs say to call off it).

**Tests**
- Unit: method names and JNI names (`_1` for `_`), the Java class's text, E0487 for each refused type.
- `tests/jni.rs` (Linux, with a JDK and `kotlinc`; skipped without them, required on CI): a Kotlin program compiled with `kotlinc` against the generated `Words.kt`, and a Java one with `javac` against `--jni-lang java`'s `Words.java`, each call a `--jni` library through the JVM: numbers, `boolean`, `char`, strings with characters outside the BMP and NUL, `null` strings, handles, a panic caught as `Words.Panic` with its message, two Java threads calling at once.
- Android: the Kotlin program, turned into DEX with the SDK's `d8` (with Kotlin's standard library), runs on CI's emulator through `app_process`, against the `x86_64-linux-android` library.

**Docs:** spec section 8 and the `--shared` row, `docs/errors/E0487.md`, the skill's C page (and a short Android section), §11 row 286, CHANGELOG, TODO, CLAUDE.md's commands, CI's system packages (a headless JDK), and `tests/kotlin.sh`, fetching a pinned Kotlin compiler checked by its SHA-256, as `tests/ndk.sh` fetches the NDK.

### Must NOT Do

- No both at once: one build writes one class's source, as an app holding both would have two `Words` classes.
- No APK, AAR or Gradle plugin: the library and the class go where the app's build expects them.
- No structs, arrays or callbacks across JNI yet.
- No change for C callers: a panic still ends the process for them.
- No JDK or NDK header needed to build the glue.

## DESIGN

- **Driver** (`src/driver/jni.rs`, new): from the export declarations, the glue's C text, the Kotlin or Java class's text, and E0487. `build_shared` with `--jni` adds the glue to the link's C files and the `Java_…` names to what the library shows.
- **The glue** (generated C, compiled with the library): for each export, `Java_<pkg>_<Class>_<method>(JNIEnv *env, void *cls, …)` converts each `jstring` to UTF-8 (`GetStringChars`, then UTF-16 → UTF-8), calls `lugha_rt_export_catch()`, the export, then `lugha_rt_export_caught()`: a message means a panic, thrown with `FindClass` + `ThrowNew`; else the result is converted back (`NewString` from UTF-16) and the C copy freed. JNI's function table is declared by the entries it uses, at their positions in the specification.
- **Runtime** (`runtime/tasks/exports.c`): `lugha_rt_export_catch()` marks the calling thread's next export call as caught; that call's task, on a panic, keeps the message for `lugha_rt_export_caught()` and ends as a task, instead of ending the process (`unwind.c`'s `panic_stop` asks the running task's call). The message is the calling thread's to free.
- **Errors:** E0487, an export Java can't call (reported by the driver, with `--jni`).
- **No new crate, no `unsafe` Rust.**
- **What a reviewer should check hardest:** the UTF-16 ↔ UTF-8 conversion (surrogate pairs, lone surrogates, NUL); every JNI string released on every path, the panic one included; JNI's function table positions against the specification; nothing of a panicked task touched after it ended.

## TESTS

- [ ] Unit (`jni.rs`): `wordCount` from `word_count`; `Java_com_example_Words_wordCount`, and `_1` for an `_` a method name keeps; the Kotlin and Java classes' text; E0487 for `u8`, an extern struct, an unsigned result.
- [ ] `tests/jni.rs`, for the Kotlin and the Java class: every type round-trips through the JVM; text with `é` and `𝄞` (outside the BMP) survives both ways, a lone surrogate arrives as U+FFFD, and NUL is refused with `IllegalArgumentException`; `null` ↔ `None`; a panic is caught as `Words.Panic` with its message, and the library still answers after; two Java threads call at once.
- [ ] `tests/exports.rs` unchanged: a C caller's panic still ends the process with 101.
- [ ] CI's Android job: the Kotlin program as DEX on the emulator.

## ACCEPTANCE

- [ ] A JVM calls a `--jni` library from Kotlin and from Java: every type, a panic as `Words.Panic`, threads.
- [ ] The same runs on CI's Android emulator.
- [ ] `cargo fmt --check`, clippy and the touched tests are clean; each file at or under 500 lines.

## DECIDED HERE (for review)

1. **The user chose Kotlin by default, Java with `--jni-lang java`,** one per build. Kotlin's `object` with `@JvmStatic external` functions and Java's `static native` methods have the same JNI names, so the glue is one for both.
2. **One class, named by `--jni`,** holding every export; methods named in lowerCamelCase after the Lugha name (not the C name), as Kotlin and Java code expect.
3. **The panic exception is `Words.Panic`,** nested in the generated class, so two Lugha libraries in one app never clash over a class name.
4. **Unsigned integers and extern structs are refused (E0487),** not mapped: Java has no unsigned types, and a struct would need a class and copies of its own; both can come later.
5. **Real UTF-8 both ways,** through UTF-16, not JNI's "modified UTF-8", which would turn NUL and characters outside the BMP into bytes Lugha's strings don't hold.
6. **JNI's function table declared by the glue itself,** from the specification's fixed positions, so building a `--jni` library needs no JDK or NDK headers, on any target.
7. **Caught panics only through the glue:** a C caller keeps PRP-303's rule, so nothing about C changes.
8. **Tested on a desktop JVM and on the emulator through `app_process`,** with no app, Gradle or APK: the same glue and library code, without Android's build tools in every test.
9. **`kotlinc` is fetched by `tests/kotlin.sh`,** pinned and checked by its SHA-256, as the NDK is: Ubuntu's archive has no current Kotlin compiler.


## MADE WHILE BUILDING (for review)

10. **NUL doesn't cross, either way:** an export's strings are C's, NUL-terminated, so a Java string holding NUL would reach Lugha cut short. The glue refuses one with `IllegalArgumentException`, and a Lugha string holding NUL panics on its way out, as it does for C. (The approved text said NUL survives both ways; it can't through C strings.)
11. **A library's runtime finds its thread-locals as a library must.** On Linux and Android the runtime reads its worker at a fixed offset from the thread pointer, which holds in an executable but not in a library opened later: a JVM's `System.loadLibrary` (`dlopen`) gives the library a block of thread-locals per thread, wherever it fits, and the first JVM run crashed there. A shared build's runtime is now compiled with `LUGHA_RT_LIBRARY`, which asks for the address on every access through a call the compiler can't fold or keep across a task switch; executables keep the fast read. C hosts linked against the library at build time never showed it, since the library then loads with the program.
12. **JNI's table positions were counted from the NDK's `jni.h`,** not from memory: `FindClass` 6, `ThrowNew` 14, `DeleteLocalRef` 23, `NewString` 163, `GetStringLength` 164, `GetStringChars` 165, `ReleaseStringChars` 166.
13. **Names Java or Kotlin reserve:** a method Java reserves is E0487 with `--jni-lang java`; Kotlin writes its own keywords in backticks (`` `object` ``) instead; a Java parameter so named gets a trailing `_`.
14. **The writer is three files:** `jni.rs` (types, names, E0487), `jni_glue.rs` (the C), `jni_source.rs` (the Kotlin and Java), as one would have passed 500 lines.
15. **CI runs `tests/jni.rs` in the build job,** which fetches `kotlinc` (`tests/kotlin.sh`, Kotlin 2.4.21) and has the JDK (`default-jdk-headless`, now a system package); the test jobs leave it out, as they do `tests/android.rs`. The build job also makes the x86-64 Android `--jni` library and the Kotlin program's jar, which the Android job turns into DEX with the SDK's `d8` and runs on the emulator through `app_process`.
