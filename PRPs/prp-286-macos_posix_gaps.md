## FEATURE: macOS's POSIX gaps: a long datagram's real length, close-on-exec pipes, multicast, the clock — milestone 252

## OBJECTIVE

Four places where Linux and macOS differ, each fixed so a program means the same on both (PRT-01). On Linux, behaviour is unchanged.

1. **A UDP datagram longer than `max`** is today silently cut short on macOS (it ignores `MSG_TRUNC` as an input flag). After: on both systems, `Err("datagram of N bytes is longer than max")` with its real length, never cut short.
2. **Starting a program** uses `pipe2`, which macOS has only from 27 (linked weakly: starting a program fails before 27). After: works on 14.4+, and no pipe of ours can leak into a program we start.
3. **Joining a multicast group** uses Linux's `struct ip_mreqn`. After: `MCAST_JOIN_GROUP`/`MCAST_LEAVE_GROUP` with `struct group_req` (an interface index), on both.
4. **Sleeping and deadlines**: macOS's `CLOCK_MONOTONIC` keeps counting while the Mac sleeps; Linux's stops. After: macOS uses `CLOCK_UPTIME_RAW`, which stops, so `task.sleep`, timeouts and deadlines mean the same on both.

## CONTEXT

- Research: `docs/research/macos.md` §3–5 (XNU source for `MSG_TRUNC`, `SO_NREAD`, `FIONREAD`, `POSIX_SPAWN_CLOEXEC_DEFAULT`; Go, Rust std, libuv, CPython).
- Read first: `runtime/os/posix/sockets.c` (`os_sock_recvfrom`), `runtime/io/udp.c`, `runtime/os/posix/process.c` 🔒 (lines 125–165: pipes and `posix_spawnp`), `runtime/os/posix/udp_options.c` (`lugha_rt_udp_group`), `runtime/tasks/tasks.c:280` and `runtime/io/random.c:35` (the clock), `runtime/os/layer.c`, `runtime/os/{linux,darwin}/os.c`.
- Implements: v18 Draft 9.3 step 4's next part [PRT-01, PRT-08]. Decisions: none open.
- Follows: PRP-287 (folder confinement, the component walk: the user's choice), PRP-288 (lockdown through Seatbelt's coarse parts: the user's choice).

## REQUIREMENTS

### Must Do

1. **UDP, both systems** (`os/posix/sockets.c`): `os_sock_recvfrom` becomes one `recvmsg` into two buffers, the caller's `max` bytes then a 64 KiB scratch area of the thread's. A UDP payload is at most 65,527 bytes, so the call returns the datagram's real length; `io/udp.c`'s "longer than max" check is unchanged. If `msg_flags` still has `MSG_TRUNC` (a datagram over 64 KiB + `max`, which UDP can't carry), it's `OS_FAILED` with "datagram is longer than 65536 bytes". `OS_RECV_FLAGS` goes from both systems' files.
2. **Pipes** (the layer): `os_pipe(fds)` gives a pipe both ends close-on-exec: Linux `pipe2(O_CLOEXEC)`; macOS `pipe` then `fcntl(FD_CLOEXEC)` on each end. `process.c` 🔒 calls it instead of `pipe2`.
3. **Spawning** (the layer): `os_spawn_prepare(&attr, &actions, share)` adds what a system needs: nothing on Linux; on macOS `POSIX_SPAWN_CLOEXEC_DEFAULT` (every descriptor not named by the file actions is closed in the child, atomically) and, when the child shares this program's stdin, stdout and stderr (`mode` 0), `posix_spawn_file_actions_addinherit_np` for 0, 1 and 2. `process.c` 🔒 calls it once, and ORs its flags with `POSIX_SPAWN_SETSIGMASK`.
4. **Multicast** (`os/posix/udp_options.c`): join and leave with `MCAST_JOIN_GROUP`/`MCAST_LEAVE_GROUP` and `struct group_req` (the group as a `sockaddr_storage`, the interface by index), for IPv4 and IPv6 alike; `IP_MULTICAST_IF` keeps its `ip_mreqn` on Linux through the layer, `ip_mreq` with the interface's address on macOS — or, if simpler, `IP_MULTICAST_IFINDEX` where both have it (decided while building, both tested).
5. **The clock** (the layer): `OS_MONOTONIC`, `CLOCK_MONOTONIC` on Linux and `CLOCK_UPTIME_RAW` on macOS, used by `now_ns` (`tasks/tasks.c`) and `lugha_rt_now_ms` (`io/random.c`).
6. **CI's Mac job** grows two cases: a UDP datagram to itself longer than `max` (the error with its length), and starting `echo` (its output captured; works on macOS 15).
7. Docs: CHANGELOG (Fixed: macOS's silent UDP truncation, starting programs on macOS), spec §11 row 252, TODO (the four removed from the macOS list), `docs/research/macos.md` noted as acted on.

### Must NOT Do

- No change to what any Linux program sees: the same results, messages and timing.
- No folder confinement or lockdown (PRP-287, PRP-288).
- No change to `process.c` 🔒 beyond the two calls above (the user's review).

## DESIGN

**Why two buffers, not `SO_NREAD` or a peek:** `SO_NREAD` then `recv` races with another reader of the same socket; a peek costs a second call and copy on every receive. One `recvmsg` with the scratch area behind the caller's buffer is atomic and costs nothing unless the datagram is too long (research §3). It also retires Linux's `MSG_TRUNC`-as-input trick, so both systems run one path.

**The scratch area** is 64 KiB per thread (`static __thread`), only written when a datagram overflows `max`; never handed to Lugha.

**Pipes and spawning (security, user's review):** `POSIX_SPAWN_CLOEXEC_DEFAULT` closes every descriptor our file actions don't name, so a program we start can never inherit one of our pipes or sockets, even in the gap between `pipe` and `fcntl`; libuv does the same. Left over, as for Go, Rust and libuv: a child started by foreign C code (`system()`) in that two-call gap could inherit a pipe; documented. Linux keeps `pipe2`, atomic.

**The clock:** Lugha's deadlines, sleeps and `monotonic_ms` measure time the program could run; on Linux a suspended machine doesn't count, and now neither does a sleeping Mac.

**Judgement calls (for review).** The 64 KiB scratch per thread; `group_req` for joins on both systems (one path, and the modern API on both); `CLOCK_UPTIME_RAW` (Linux's meaning) over `CLOCK_MONOTONIC`; two new Mac cases rather than the whole suite (that needs folders, PRP-287).

## TESTS

- [ ] `tests/udp.rs` (Linux): a datagram longer than `max` still gives its real length (existing case, now through `recvmsg`); one of exactly `max` bytes isn't refused; multicast join and leave (existing cases, now `group_req`).
- [ ] `src/link/mod.rs`: the boundary test still holds; `pipe2` named only in `os/linux/`.
- [ ] `tests/macos.rs`: the new Mac cases build; the macOS runtime compiles with zig with no `pipe2` warning.
- [ ] CI's Mac job: the two new cases' output.
- [ ] Linux unchanged: `--test udp`, `net`, `helpers`, `lockdown` (programs started), `test_capabilities`, `programs`, `scheduler_stress`, `workers`, `arm`, `musl`, `--lib`; benchmarks.

## ACCEPTANCE

- [ ] The four behaviours above; CI's Mac job green with the new cases.
- [ ] Tests pass; fmt and clippy clean; files ≤ 500 lines.
- [ ] CHANGELOG, spec row 252, TODO updated.
