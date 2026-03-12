use std::process::{Child, Command, Stdio};

const PERF_SCRIPT: &str = r#"
import sys, time, os, signal, shutil, re

PID     = int(sys.argv[1])
CMD     = (sys.argv[2] if len(sys.argv) > 2 else "sandbox")[:22]

USER_HZ  = os.sysconf(os.sysconf_names["SC_CLK_TCK"])
INTERVAL = 0.5
HISTORY  = 60
SPARK    = " ▁▂▃▄▅▆▇█"
_ANSI    = re.compile(r"\033\[[0-9;]*m")

# ── proc readers ─────────────────────────────────────────────────────────────

def get_process_tree(root_pid):
    """Find all recursive children of root_pid by scanning /proc."""
    pids = {root_pid}
    try:
        # Build parent -> children map
        children = {}
        for p in os.listdir("/proc"):
            if p.isdigit():
                try:
                    with open(f"/proc/{p}/stat") as f:
                        fields = f.read().split()
                        ppid = int(fields[3])
                        if ppid not in children: children[ppid] = []
                        children[ppid].append(int(p))
                except: continue
        
        # Recursive collect
        stack = [root_pid]
        while stack:
            curr = stack.pop()
            if curr in children:
                for child in children[curr]:
                    if child not in pids:
                        pids.add(child)
                        stack.append(child)
    except: pass
    return pids

def read_aggregated_ticks(pids):
    total = 0
    for pid in pids:
        try:
            with open(f"/proc/{pid}/stat") as f:
                fields = f.read().split()
                total += int(fields[13]) + int(fields[14])
        except: continue
    return total

def read_aggregated_state(pids):
    states = set()
    for pid in pids:
        try:
            with open(f"/proc/{pid}/stat") as f:
                states.add(f.read().split()[2])
        except: continue
    if "R" in states: return "R"
    if "D" in states: return "D"
    if "S" in states: return "S"
    return "?" if not states else list(states)[0]

def read_aggregated_mem(pids):
    rss = vmsz = threads = 0
    for pid in pids:
        try:
            with open(f"/proc/{pid}/status") as f:
                for line in f:
                    if   line.startswith("VmRSS:"):    rss     += int(line.split()[1])
                    elif line.startswith("VmSize:"):   vmsz    += int(line.split()[1])
                    elif line.startswith("Threads:"):  threads += int(line.split()[1])
        except: continue
    return rss, vmsz, threads

def read_aggregated_io(pids):
    rb = wb = 0
    for pid in pids:
        try:
            with open(f"/proc/{pid}/io") as f:
                for line in f:
                    if   line.startswith("read_bytes:"):  rb += int(line.split()[1])
                    elif line.startswith("write_bytes:"): wb += int(line.split()[1])
        except: continue
    return rb, wb

# ── layout helpers ────────────────────────────────────────────────────────────

def get_W():
    return max(72, min(shutil.get_terminal_size((80, 24)).columns, 96))

def vlen(s):
    return len(_ANSI.sub("", s))

def row(content, W):
    vis = vlen(content)
    avail = W - 4
    if vis > avail:
        content = _ANSI.sub("", content)[:avail]
        vis = avail
    pad = avail - vis
    return f"\033[1;34m║\033[0m {content}{' ' * pad} \033[1;34m║\033[0m"

def blank(W):
    return row("", W)

def div(W):
    return f"\033[1;34m╠{'═' * (W - 2)}╣\033[0m"

def top(W):
    title  = " LION  PERF  MONITOR "
    inner  = W - 2
    pad_l  = (inner - len(title)) // 2
    pad_r  = inner - len(title) - pad_l
    return (f"\033[1;34m╔{'═' * pad_l}"
            f"\033[1;36m{title}"
            f"\033[1;34m{'═' * pad_r}╗\033[0m")

def bot(W):
    return f"\033[1;34m╚{'═' * (W - 2)}╝\033[0m"

# ── value formatters ──────────────────────────────────────────────────────────

def pct_c(p):
    if p > 80: return "\033[1;31m"
    if p > 50: return "\033[1;33m"
    return "\033[1;32m"

def fmt_rate(n):
    for unit in ("B/s", "KB/s", "MB/s", "GB/s"):
        if n < 1024:
            return f"{n:6.1f} {unit}"
        n /= 1024
    return f"{n:6.1f} TB/s"

def gradient_bar(pct, BW):
    g = int(BW * 0.60)
    y = int(BW * 0.80)
    n = max(0, min(BW, int(pct / 100.0 * BW)))
    b = ""
    for i in range(BW):
        ch = "█" if i < n else "░"
        if i < n:
            c = "\033[32m" if i < g else ("\033[33m" if i < y else "\033[31m")
        else:
            c = "\033[90m"
        b += c + ch + "\033[0m"
    return b

def sparkline(hist, maxv=None):
    m = maxv if maxv else (max(hist) if hist else 1)
    m = m or 1
    return "".join(SPARK[min(8, int(v / m * 8))] for v in hist)

def state_badge(s):
    return {
        "R": "\033[1;32m● RUNNING \033[0m",
        "S": "\033[90m○ IDLE    \033[0m",
        "D": "\033[1;33m◎ DISKWAIT\033[0m",
        "Z": "\033[1;31m✖ ZOMBIE  \033[0m",
        "T": "\033[1;33m‖ STOPPED \033[0m",
    }.get(s, f"\033[90m? {s}\033[0m")

# ── screen control ────────────────────────────────────────────────────────────

def enter_alt(): sys.stdout.write("\033[?1049h\033[?25l"); sys.stdout.flush()
def leave_alt(): sys.stdout.write("\033[?1049l\033[?25h"); sys.stdout.flush()
def home():      sys.stdout.write("\033[H")
def cls():       sys.stdout.write("\033[2J\033[H")

def cleanup(*_): leave_alt(); sys.exit(0)
signal.signal(signal.SIGTERM, cleanup)
signal.signal(signal.SIGINT,  cleanup)

# ── state ─────────────────────────────────────────────────────────────────────

cpu_hist   = [0.0] * HISTORY
mem_hist   = [0.0] * HISTORY
peak_cpu   = 0.0
peak_mem   = 0.0
sample     = 0
t0         = time.monotonic()

active_pids = get_process_tree(PID)
prev_ticks  = read_aggregated_ticks(active_pids)
prev_time   = time.monotonic()
prev_rb, prev_wb = read_aggregated_io(active_pids)

enter_alt(); cls()
time.sleep(INTERVAL)

# ── main loop ─────────────────────────────────────────────────────────────────

try:
    while True:
        now         = time.monotonic()
        active_pids = get_process_tree(PID)
        
        # ── exit-detection ────────────────────────────────────────────────────
        if not os.path.exists(f"/proc/{PID}"):
            W = get_W()
            home()
            sys.stdout.write(
                "\n".join([
                    top(W), blank(W),
                    row(f"\033[90m  PID {PID} has exited — monitoring stopped.\033[0m", W),
                    blank(W), bot(W), "",
                ]))
            sys.stdout.flush()
            sys.stdout.flush()
            break

        cur_ticks = read_aggregated_ticks(active_pids)

        # ── CPU ──────────────────────────────────────────────────────────────
        elapsed    = max(now - prev_time, 1e-4)
        cpu_pct    = max(0.0, min(
                        100.0 * (os.cpu_count() or 1),
                        (cur_ticks - prev_ticks) / USER_HZ / elapsed * 100.0))
        prev_ticks = cur_ticks
        prev_time  = now

        # ── memory ────────────────────────────────────────────────────────────
        rss_kb, vsz_kb, threads = read_aggregated_mem(active_pids)
        mem_mb = rss_kb / 1024.0
        vsz_mb = vsz_kb / 1024.0

        # ── I/O rates ─────────────────────────────────────────────────────────
        cur_rb, cur_wb = read_aggregated_io(active_pids)
        io_r = max(0, cur_rb - prev_rb) / elapsed
        io_w = max(0, cur_wb - prev_wb) / elapsed
        prev_rb, prev_wb = cur_rb, cur_wb

        # ── history ───────────────────────────────────────────────────────────
        cpu_hist = (cpu_hist + [cpu_pct])[-HISTORY:]
        mem_hist = (mem_hist + [mem_mb])[-HISTORY:]
        peak_cpu = max(peak_cpu, cpu_pct)
        peak_mem = max(peak_mem, mem_mb)
        sample  += 1

        uptime = int(now - t0)
        up_str = f"{uptime // 60}m {uptime % 60:02d}s"
        state  = read_aggregated_state(active_pids)

        # ── layout constants ──────────────────────────────────────────────────
        W  = get_W()
        BW = W - 34
        SW = BW

        # ── bars ──────────────────────────────────────────────────────────────
        cpu_bar  = gradient_bar(cpu_pct, BW)
        mem_axis = max(peak_mem * 1.2, mem_mb + 10.0, 64.0)
        mem_pct  = min(100.0, mem_mb / mem_axis * 100.0)
        mem_bar  = gradient_bar(mem_pct, BW)

        csp = sparkline(cpu_hist)[-SW:]
        msp = sparkline(mem_hist, mem_axis)[-SW:]

        cc  = pct_c(cpu_pct)
        mc  = pct_c(mem_pct)
        pkc = pct_c(peak_cpu)

        lines = [
            top(W),
            row(f" pid \033[1m{PID:<7}\033[0m"
                f"  cmd \033[1;36m{CMD:<22}\033[0m"
                f"  {state_badge(state)}"
                f"  \033[90m▲ {up_str}\033[0m", W),
            div(W),
            row(f"\033[1mCPU\033[0m  [{cpu_bar}]  {cc}{cpu_pct:5.1f}%\033[0m  pk {pkc}{peak_cpu:5.1f}%\033[0m", W),
            row(f"     \033[90m{csp}\033[0m"
                f"  \033[90m· last {HISTORY // 2}s"
                f"  · thr \033[0m\033[1m{threads}\033[0m\033[90m"
                f" / {os.cpu_count()} cpus\033[0m", W),
            div(W),
            row(f"\033[1mMEM\033[0m  [{mem_bar}]  {mc}{mem_mb:5.0f}M\033[0m  pk \033[90m{peak_mem:5.0f}M\033[0m", W),
            row(f"     \033[90m{msp}\033[0m"
                f"  \033[90m· rss \033[0m\033[1m{mem_mb:6.1f} MB\033[0m"
                f"  \033[90mpids: \033[0m\033[1m{len(active_pids)}\033[0m", W),
            div(W),
            row(f"\033[1mI/O\033[0m  "
                f"\033[90m↓\033[0m \033[1;32m{fmt_rate(io_r)}\033[0m  "
                f"\033[90m↑\033[0m \033[1;33m{fmt_rate(io_w)}\033[0m", W),
            div(W),
            row(f"\033[90m sample \033[0m\033[1m#{sample:<5}\033[0m"
                f"\033[90m  ·  {INTERVAL*1000:.0f}ms interval"
                f"  ·  Ctrl-C to close\033[0m", W),
            bot(W),
            "",
        ]

        home()
        sys.stdout.write("\n".join(lines))
        sys.stdout.flush()
        time.sleep(INTERVAL)

finally:
    leave_alt()
"#;

/// Handle to the perf monitor terminal window.
/// Killed automatically on drop.
pub struct PerfHandle {
    child: Option<Child>,
}

impl PerfHandle {
    /// Spawn the perf monitor in a separate terminal window watching `pid`.
    /// Returns None silently if no supported terminal is found.
    pub fn spawn(pid: u32, cmd_label: &str) -> Option<Self> {
        let script_path = std::env::temp_dir().join(format!("lion_perf_{}.py", pid));
        std::fs::write(&script_path, PERF_SCRIPT).ok()?;

        let script_str = script_path.to_string_lossy().to_string();
        // No trailing 'read' prompt — the Python script already shows a
        // "PID exited" banner before it exits, then the terminal
        // auto-closes. We use 'exec' so the terminal process *is* the python process.
        let lion_cmd = format!("exec python3 {} {} {}", script_str, pid, cmd_label);

        let terminals: &[(&str, &[&str])] = &[
            // --wait keeps gnome-terminal foreground so our Child handle is live
            ("gnome-terminal", &["--wait", "--"]),
            ("kitty",          &[]),
            ("xterm",          &["-e"]),
            ("konsole",        &["-e"]),
            ("xfce4-terminal", &["--command"]),
        ];

        for (term, prefix_args) in terminals {
            let mut cmd = Command::new(term);
            for a in prefix_args.iter() {
                cmd.arg(a);
            }
            // gnome-terminal needs bash -c "...", others take the command directly
            if *term == "gnome-terminal" {
                cmd.arg("bash").arg("-c").arg(&lion_cmd);
            } else {
                cmd.arg("bash").arg("-c").arg(&lion_cmd);
            }
            cmd.stdout(Stdio::null()).stderr(Stdio::null());
            if let Ok(child) = cmd.spawn() {
                return Some(PerfHandle { child: Some(child) });
            }
        }

        None
    }
}

impl Drop for PerfHandle {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
