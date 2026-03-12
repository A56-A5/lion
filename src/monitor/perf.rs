use std::process::{Child, Command, Stdio};

const PERF_SCRIPT: &str = r#"
import sys, time, os

PID = int(sys.argv[1])
CMD = sys.argv[2] if len(sys.argv) > 2 else "sandbox"

BAR_WIDTH = 30

def read_proc_stat(pid):
    try:
        with open(f"/proc/{pid}/stat") as f:
            fields = f.read().split()
        utime = int(fields[13])
        stime = int(fields[14])
        return utime + stime
    except:
        return None

def read_system_cpu():
    try:
        with open("/proc/stat") as f:
            line = f.readline()
        vals = list(map(int, line.split()[1:]))
        return sum(vals), vals[3] + vals[4]  # total, idle
    except:
        return None, None

def read_mem_kb(pid):
    try:
        with open(f"/proc/{pid}/status") as f:
            for line in f:
                if line.startswith("VmRSS:"):
                    return int(line.split()[1])
    except:
        pass
    return 0

def bar(pct, width=BAR_WIDTH):
    filled = int(pct / 100 * width)
    filled = max(0, min(width, filled))
    b = "█" * filled + "░" * (width - filled)
    if pct > 80:
        color = "\033[1;31m"  # red
    elif pct > 50:
        color = "\033[1;33m"  # yellow
    else:
        color = "\033[1;32m"  # green
    return f"{color}{b}\033[0m"

def mem_bar(mb, max_mb=512):
    pct = min(100, mb / max_mb * 100)
    return bar(pct)

os.system("clear")
print(f"\033[1;34m╔══════════════════════════════════════════════════╗\033[0m")
print(f"\033[1;34m║  LION PERF MONITOR  ·  {CMD:<27}║\033[0m")
print(f"\033[1;34m╚══════════════════════════════════════════════════╝\033[0m")
print()

prev_proc = read_proc_stat(PID)
prev_total, prev_idle = read_system_cpu()
time.sleep(0.5)

cpu_history = [0.0] * 40
mem_history = [0]

while True:
    cur_proc = read_proc_stat(PID)
    cur_total, cur_idle = read_system_cpu()

    if cur_proc is None:
        print("\n\033[90m[LION] process exited — perf monitor stopped\033[0m")
        break

    # CPU %
    if prev_proc is not None and cur_total and prev_total:
        delta_proc = cur_proc - prev_proc
        delta_total = cur_total - prev_total
        delta_idle = cur_idle - prev_idle
        cpu_pct = (delta_proc / max(delta_total, 1)) * 100
        cpu_pct = max(0.0, min(100.0, cpu_pct))
    else:
        cpu_pct = 0.0

    prev_proc = cur_proc
    prev_total = cur_total
    prev_idle = cur_idle

    mem_kb = read_mem_kb(PID)
    mem_mb = mem_kb / 1024

    cpu_history.append(cpu_pct)
    cpu_history = cpu_history[-40:]
    mem_history.append(mem_mb)
    mem_history = mem_history[-40:]

    # Build spark line (mini graph using block chars)
    spark_chars = " ▁▂▃▄▅▆▇█"
    max_cpu = max(cpu_history) if max(cpu_history) > 0 else 1
    spark = "".join(spark_chars[min(8, int(v / max_cpu * 8))] for v in cpu_history)

    # Move cursor up to redraw (ANSI: cursor up N lines)
    print("\033[8A", end="")  # move up 8 lines

    print(f"  PID : \033[1m{PID}\033[0m     command: \033[1;36m{CMD[:30]}\033[0m          ")
    print()
    print(f"  CPU  {bar(cpu_pct)}  \033[1m{cpu_pct:5.1f}%\033[0m  ")
    print(f"       \033[90m{spark}\033[0m history (last 20s)  ")
    print()
    max_mem = max(mem_history) if max(mem_history) > 0 else 1
    print(f"  MEM  {mem_bar(mem_mb)}  \033[1m{mem_mb:6.1f} MB\033[0m")
    spark_mem = "".join(spark_chars[min(8, int(v / max(max_mem, 1) * 8))] for v in mem_history)
    print(f"       \033[90m{spark_mem}\033[0m history (last 20s)  ")
    print()
    print(f"  \033[90mupdating every 500ms · q to quit\033[0m                        ")

    time.sleep(0.5)
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
