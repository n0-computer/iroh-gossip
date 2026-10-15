import json, sys, collections
rows = [json.loads(l) for l in open(sys.argv[1]) if l.strip()]
for r in rows:
    # The slow group overwrote the slow count in round 1.
    if isinstance(r.get("slow"), dict):
        r["slow_n"] = r["slow"]["nodes"]
        r["slow_nodes"] = r["slow"]
        r["healthy_nodes"] = r.get("healthy", {})
    else:
        r["slow_n"] = r.get("slow")
order = ["main", "stalls", "split", "fifo", "peerq"]
cells = collections.defaultdict(list)
for r in rows:
    cells[(r["slow_n"], r["rate"])].append(r)
def g(r, grp, key, fmt="{:.2f}"):
    v = r.get(grp, {}).get(key)
    return "-" if v is None else fmt.format(v)
hdr = f"{'slow':>4} {'rate':>4} {'rev':>6} | {'pub/s':>6} {'lateMs':>7} | {'H dlv':>5} {'H p50':>6} {'H p99':>7} | {'S dlv':>5} {'S min':>5} {'S p50':>7} {'S p99':>8} {'S late':>6} | {'drop':>6} {'qfull':>5} {'nDown':>5} {'rss':>5}"
print(hdr)
for key in sorted(cells, key=lambda k: tuple(str(x) for x in k)):
    rs = sorted(cells[key], key=lambda r: order.index(r["label"]) if r["label"] in order else 99)
    for r in rs:
        if r.get("failed"):
            print(f"{key[0]:>4} {key[1]:>4} {r['label']:>6} | FAILED"); continue
        m = r.get("metrics", {})
        print(f"{r['slow_n']:>4} {r['rate']:>4} {r['label']:>6} | {r['achieved_rate']:>6.1f} {r['max_publish_lateness_ms']:>7.0f} | "
              f"{g(r,'healthy_nodes','delivered_mean'):>5} {g(r,'healthy_nodes','age_p50_ms','{:.0f}'):>6} {g(r,'healthy_nodes','age_p99_ms','{:.0f}'):>7} | "
              f"{g(r,'slow_nodes','delivered_mean'):>5} {g(r,'slow_nodes','delivered_min'):>5} {g(r,'slow_nodes','age_p50_ms','{:.0f}'):>7} {g(r,'slow_nodes','age_p99_ms','{:.0f}'):>8} {r.get('slow_nodes',{}).get('after_publish','-'):>6} | "
              f"{m.get('msgs_dropped','-'):>6} {m.get('peers_send_queue_full','-'):>5} {r['neighbor_down']:>5} {r['max_rss_mb']:>5.0f}")
    print()
