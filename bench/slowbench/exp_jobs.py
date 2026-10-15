import itertools, random, shlex
variants = {
  "fifo": ("exp", "", ""),
  "headdrop-sw": ("exp", "GOSSIP_EXP_HEADDROP=1", "--send-window 131072"),
  "hp-sw": ("exp", "GOSSIP_EXP_HEADDROP=1 GOSSIP_EXP_PRIO=1", "--send-window 131072"),
  "hp-rw": ("exp", "GOSSIP_EXP_HEADDROP=1 GOSSIP_EXP_PRIO=1", "--receive-window 131072"),
  "hp-sw64k": ("exp", "GOSSIP_EXP_HEADDROP=1 GOSSIP_EXP_PRIO=1", "--send-window 65536"),
  "hp-sw256k": ("exp", "GOSSIP_EXP_HEADDROP=1 GOSSIP_EXP_PRIO=1", "--send-window 262144"),
}
scenarios = {
  "s1": "--slow 1 --rate 250 --publish-secs 60 --drain-secs 30",
  "s8": "--slow 8 --rate 250 --publish-secs 60 --drain-secs 30",
  "burst": "--slow 1 --rate 1000 --publish-secs 5 --drain-secs 60",
  "fast": "--slow 0 --rate 2000 --publish-secs 30 --drain-secs 15 --rtt-ms 200",
}
import sys
reps = int(sys.argv[1]) if len(sys.argv) > 1 else 1
out = sys.argv[2] if len(sys.argv) > 2 else "/cache/dewy-bench/results/exp1.jsonl"
jobs = []
import os
os.makedirs(out, exist_ok=True)
n = 0
for rep in range(reps):
    for (v, (b, env, extra)), (s, sargs) in itertools.product(variants.items(), scenarios.items()):
        f = f"{out}/{v}_{s}_{rep}.json"
        jobs.append(f"env {env} timeout 400 /cache/dewy-bench/bins/{b} --label {v}:{s} --nodes 10 {sargs} {extra} > {f} 2>/dev/null || echo '{{\"label\":\"{v}:{s}\",\"failed\":true}}' > {f}")
random.seed(7)
random.shuffle(jobs)
print("\n".join(jobs))
