use std::process::{Child, Command, Stdio};

const PERF_SCRIPT: &str = r#"
import sys, time, os, signal, shutil

PID   = int(sys.argv[1])
CMD   = sys.argv[2] if len(sys.argv) > 2 else "sandbox"

# Clock ticks per second (almost always 100 on Linux)
USER_HZ = os.sysconf(os.sysconf_names["SC_CLK_TCK"])

INTERVAL   = 0.5          # seconds between samples
HISTORY    = 60           # number of history samples kept (~30s at 0.5s)
BAR_WIDTH  = 36
SPARK_CHARS = " ▁▂▃▄▅▆▇█"

# ── proc readers ────────────────────────────────────────────────────────────

def read_proc_ticks(pid):
    """Return utime+stime (in clock ticks) for the process, or None if gone."""
    try:
        with open(f"/proc/{pid}/stat") as f:
            fields = f.read().split()
        return int(fields[13]) + int(fields[14])
    except:
        return None

def read_proc_state(pid):
    """Return single-char state letter from /proc/PID/stat."""
    try:
        with open(f"/proc/{pid}/stat") as f:
            fields = f.read().split()
        return fields[2]
    except:
        return "?"

def read_mem_status(pid):
    """Return (rss_kb, vmsize_kb, threads) from /proc/PID/status."""
    rss = vmsz = threads = 0
    try:
        with open(f"/proc/{pid}/status") as f:
            for line in f:
                if line.startswith("VmRSS:"):
                    rss = int(line.split()[1])
                elif line.startswith("VmSize:"):
                    vmsz = int(line.split()[1])
                elif line.startswith("Threads:"):
                    threads = int(line.split()[1])
    except:
        pass
    return rss, vmsz, threads

def read_io(pid):
    """Return (read_bytes, write_bytes) cumulative from /proc/PID/io."""
    rb = wb = 0
    try:
        with open(f"/proc/{pid}/io") as f:
            for line in f:
                if line.startswith("read_bytes:"):
                    rb = int(line.split()[1])
                elif line.startswith("write_bytes:"):
                    wb = int(line.split()[1])
    except:
        pass
    return rb, wb

# ── formatting helpers ───────────────────────────────────────────────────────

def gradient_bar(pct, width=BAR_WIDTH):
    """Render a filled bar whose colour shifts green → yellow → red with load."""
    filled = max(0, min(width, int(pct / 100 * width)))
    # segment colouring: first 60 % green, next 20 % yellow, rest red
    green_end  = int(width * 0.60)
    yellow_end = int(width * 0.80)
    bar = ""
    for i in range(width):
        ch = "█" if i < filled else "░"
        if i < filled:
            if i < green_end:
                bar += f"\033[32m{ch}\033[0m"
            elif i < yellow_end:
                bar += f"\033[33m{ch}\033[0m"
            else:
                bar += f"\033[31m{ch}\033[0m"
        else:
            bar += f"\033[90m{ch}\033[0m"
    return bar

def pct_color(pct):
    if pct > 80: return "\033[1;31m"
    if pct > 50: return "\033[1;33m"
    return "\033[1;32m"

def fmt_bytes(n):
    for unit in ("B", "KB", "MB", "GB"):
        if n < 1024:
            return f"{n:.1f} {unit}"
        n /= 1024
    return f"{n:.1f} TB"

def sparkline(history, max_val=None):
    m = max_val if max_val else (max(history) if history else 1)
    m = m or 1
    return "".join(SPARK_CHARS[min(8, int(v / m * 8))] for v in history)

def state_label(s):
    return {
        "R": "\033[1;32mRUNNING\033[0m",
        "S": "\033[1;34mSLEEPING\033[0m",
        "D": "\033[1;33mDISK WAIT\033[0m",
        "Z": "\033[1;31mZOMBIE\033[0m",
        "T": "\033[1;33mSTOPPED\033[0m",
    }.get(s, f"\033[90m{s}\033[0m")

def w():
    """Terminal width, capped sensibly."""
    return min(shutil.get_terminal_size((80, 24)).columns, 100)

def box_top(title, width):
    inner = width - 2
    side  = (inner - len(title) - 2) // 2
    return (f"\033[1;34m╔{'═' * side} \033[1;36m{title}\033[1;34m "
            f"{'═' * (inner - side - len(title) - 2)}╗\033[0m")

def box_bot(width):
    return f"\033[1;34m╚{'═' * (width - 2)}╝\033[0m"

def box_row(content, width):
    # Strip ANSI for length calculation
    import re
    plain = re.sub(r"\033\[[0-9;]*m", "", content)
    pad = width - 2 - len(plain)
    return f"\033[1;34m║\033[0m {content}{' ' * max(0, pad - 1)}\033[1;34m║\033[0m"

# ── alternate-screen helpers ─────────────────────────────────────────────────

def enter_alt():
    sys.stdout.write("\033[?1049h\033[?25l")
    sys.stdout.flush()

def leave_alt():
    sys.stdout.write("\033[?1049l\033[?25h")
    sys.stdout.flush()

def home():
    sys.stdout.write("\033[H")

def clear_screen():
    sys.stdout.write("\033[2J\033[H")

# ── clean exit ───────────────────────────────────────────────────────────────

def cleanup(*_):
    leave_alt()
    sys.exit(0)

signal.signal(signal.SIGTERM, cleanup)
signal.signal(signal.SIGINT,  cleanup)

# ── main ─────────────────────────────────────────────────────────────────────

enter_alt()
clear_screen()

cpu_history = [0.0] * HISTORY
mem_history = [0.0] * HISTORY
peak_cpu    = 0.0
peak_mem    = 0.0
sample      = 0
start_time  = time.monotonic()

# Seed ticks + io for first delta
prev_ticks    = read_proc_ticks(PID) or 0
prev_time     = time.monotonic()
prev_rb, prev_wb = read_io(PID)

time.sleep(INTERVAL)

try:
    while True:
        now_time  = time.monotonic()
        cur_ticks = read_proc_ticks(PID)

        if cur_ticks is None:
            home()
            W = w()
            print()
            print(f"  \033[90m[LION] PID {PID} has exited — monitoring stopped.\033[0m")
            time.sleep(3)
            break

        # ── CPU: wall-clock method (correct across any number of cores) ──────
        elapsed   = now_time - prev_time
        elapsed   = elapsed if elapsed > 0 else INTERVAL
        delta_t   = cur_ticks - prev_ticks
        cpu_pct   = max(0.0, (delta_t / USER_HZ / elapsed) * 100.0)
        # Cap at 100 % per core; on multi-threaded workloads can exceed 100
        cpu_pct   = min(cpu_pct, 100.0 * max(1, os.cpu_count() or 1))

        prev_ticks = cur_ticks
        prev_time  = now_time

        # ── Memory ───────────────────────────────────────────────────────────
        rss_kb, vsz_kb, threads = read_mem_status(PID)
        mem_mb  = rss_kb / 1024.0
        vsz_mb  = vsz_kb / 1024.0

        # ── I/O rates ────────────────────────────────────────────────────────
        cur_rb, cur_wb = read_io(PID)
        io_r_rate = max(0, cur_rb - prev_rb) / elapsed
        io_w_rate = max(0, cur_wb - prev_wb) / elapsed
        prev_rb, prev_wb = cur_rb, cur_wb

        # ── History ──────────────────────────────────────────────────────────
        cpu_history.append(cpu_pct)
        cpu_history = cpu_history[-HISTORY:]
        mem_history.append(mem_mb)
        mem_history = mem_history[-HISTORY:]

        peak_cpu = max(peak_cpu, cpu_pct)
        peak_mem = max(peak_mem, mem_mb)
        sample  += 1

        uptime = int(now_time - start_time)
        up_str = f"{uptime // 60}m {uptime % 60:02d}s"

        # ── Render ───────────────────────────────────────────────────────────
        W   = min(w(), 60)
        state = read_proc_state(PID)

        home()

        print(box_top("LION  PERF  MONITOR", W))
        print(box_row(f"pid \033[1m{PID}\033[0m  cmd \033[1;36m{CMD[:24]}\033[0m  up \033[90m{up_str}\033[0m  state {state_label(state)}", W))
        print(box_row("", W))

        # CPU section
        cpu_c = pct_color(cpu_pct)
        print(box_row(f"\033[1mCPU\033[0m  {gradient_bar(cpu_pct, BAR_WIDTH)}  {cpu_c}{cpu_pct:5.1f}%\033[0m  peak \033[90m{peak_cpu:.1f}%\033[0m", W))
        cpu_spark = sparkline(cpu_history)
        print(box_row(f"     \033[90m{cpu_spark}  ·  last {HISTORY//2}s\033[0m", W))
        print(box_row(f"     threads \033[1m{threads}\033[0m   cpus \033[90m{os.cpu_count()}\033[0m", W))
        print(box_row("", W))

        # Memory section
        max_mem_axis = max(peak_mem * 1.2, mem_mb + 10, 64)
        mem_pct = min(100.0, mem_mb / max_mem_axis * 100.0)
        mem_c   = pct_color(mem_pct)
        print(box_row(f"\033[1mMEM\033[0m  {gradient_bar(mem_pct, BAR_WIDTH)}  {mem_c}{mem_mb:6.1f} MB\033[0m  peak \033[90m{peak_mem:.1f} MB\033[0m", W))
        mem_spark = sparkline(mem_history, max_mem_axis)
        print(box_row(f"     \033[90m{mem_spark}  ·  last {HISTORY//2}s\033[0m", W))
        print(box_row(f"     rss \033[1m{mem_mb:.1f} MB\033[0m  virt \033[90m{vsz_mb:.1f} MB\033[0m", W))
        print(box_row("", W))

        # I/O section
        print(box_row(f"\033[1mI/O\033[0m  read  \033[1;32m{fmt_bytes(io_r_rate):>10}/s\033[0m   write  \033[1;33m{fmt_bytes(io_w_rate):>10}/s\033[0m", W))
        print(box_row("", W))

        print(box_row(f"\033[90msample #{sample}  ·  Ctrl-C to close\033[0m", W))
        print(box_bot(W))

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
        let lion_cmd = format!("python3 {} {} {}; echo; read -p '[press enter to close]'",
            script_str, pid, cmd_label);

        let terminals: &[(&str, &[&str])] = &[
            ("gnome-terminal", &["--"]),
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
