import json, glob, sys, collections
d = sys.argv[1]
rows = []
for f in sorted(glob.glob(f"{d}/*.json")):
    try:
        rows.append(json.load(open(f)))
    except Exception as e:
        rows.append({"label": f.split("/")[-1], "failed": True})
order = ["fifo","hp","moq-reset","moq-reset-250ms","moq-wait","moq-wait-500ms","fifo-2s","fifo-lanes","hp","hp-2s","h-lanes","hp-rw256k","hp-sw256k","h-rw256k","headdrop-sw","hp-sw","hp-rw","hp-sw64k","hp-sw256k","q64k","q256k","dcap64k","dcap64k-prio","dcap64k-head","q64k-prio","prio","headdrop","q4m","norepair","fifo-sw","prio-sw","split","split-sw"]
by = collections.defaultdict(list)
for r in rows:
    v, s = r["label"].split(":") if ":" in r["label"] else (r["label"], "?")
    by[s].append((v, r))
def wait(m, c):
    b = [m.get(f"qwait_{c}_{x}", 0) for x in ["lt10ms","lt100ms","lt1s","lt10s","ge10s"]]
    n = sum(b)
    if n == 0: return "-", "-"
    return f"{m.get(f'qwait_{c}_ms_sum',0)/n:.0f}", f"{(b[3]+b[4])/n*100:.1f}%"
for s in ["s1","s8","s9","burst","fast","multi"]:
    print(f"== {s}")
    print(f"{'variant':>12} | {'pub/s':>5} | {'H p99':>6} {'H dlv':>5} {'Hpub':>4} | {'S msgs':>6} {'S min':>5} {'S p50s':>6} {'S p99s':>6} {'late':>5} | {'drops':>7} {'qfull':>5} {'nDown':>5} | {'cW ms':>6} {'c>1s':>6} {'dW ms':>6} | {'rss':>4}")
    for v, r in sorted(by[s], key=lambda x: order.index(x[0]) if x[0] in order else 99):
        if r.get("failed"):
            print(f"{v:>12} | FAILED"); continue
        m = r.get("metrics", {})
        h = r.get("healthy_nodes", {}); sl = r.get("slow_nodes", {})
        hpub = [n.get("publisher_neighbor") for n in r.get("per_node", [])[1:] if not n["slow"]]
        hp = "".join("y" if x else "n" for x in hpub)
        cw, c1 = wait(m, "ctrl"); dw, _ = wait(m, "data")
        sent = r["sent"]
        if not sl:
            print(f"{v:>12} | {r['achieved_rate']:>5.0f} | {h.get('age_p99_ms', float('nan')):>6.0f} {h.get('delivered_min', float('nan')):>5.2f} | p50 {h.get('age_p50_ms', float('nan')):.0f} ms | drops {m.get('msgs_dropped',0)} qfull {m.get('peers_send_queue_full',0)} nDown {r['neighbor_down']} | cW {cw} dW {dw}")
            continue
        print(f"{v:>12} | {r['achieved_rate']:>5.0f} | {h.get('age_p99_ms', float('nan')):>6.0f} {h.get('delivered_min', float('nan')):>5.2f} {hp:>4} | "
              f"{sl['delivered_total']/sl['nodes']:>6.0f} {sl['delivered_min']:>5.2f} {sl['age_p50_ms']/1000:>6.1f} {sl['age_p99_ms']/1000:>6.1f} {sl['after_publish']/sl['nodes']:>5.0f} | "
              f"{m.get('msgs_dropped',0):>7} {m.get('peers_send_queue_full',0):>5} {r['neighbor_down']:>5} | {cw:>6} {c1:>6} {dw:>6} | {r['max_rss_mb']:>4.0f}")
    print(f"   (sent per run: {sorted(set(r['sent'] for _, r in by[s] if not r.get('failed')))})")
