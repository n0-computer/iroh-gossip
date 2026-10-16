import json, glob, collections, sys

d = sys.argv[1] if len(sys.argv) > 1 else '/cache/dewy-bench/results/exp8'
variants = sys.argv[2].split(',') if len(sys.argv) > 2 else ["opt-complete", "opt-newest"]
rows = collections.defaultdict(list)
for f in glob.glob(f'{d}/*.json'):
    try:
        r = json.load(open(f))
    except Exception:
        print('bad', f)
        continue
    if r.get('failed'):
        print('FAILED', f)
        continue
    v, s = r['label'].split(':')
    rows[(s, v)].append(r)


def L(rs, f, fmt="{:.0f}"):
    return "/".join(fmt.format(f(r)) for r in rs)


for s in ["s1", "s8", "multi"]:
    print("==", s)
    for v in variants:
        rs = rows[(s, v)]
        print(f"  {v:>13}: slow msgs {L(rs, lambda r: r['slow_nodes']['delivered_total'] / r['slow_nodes']['nodes']):>10}"
              f" | p50 s {L(rs, lambda r: r['slow_nodes']['age_p50_ms'] / 1000, '{:.1f}'):>9}"
              f" | p99 s {L(rs, lambda r: r['slow_nodes']['age_p99_ms'] / 1000, '{:.1f}'):>10}"
              f" | H p99 ms {L(rs, lambda r: r.get('healthy_nodes', {}).get('age_p99_ms', float('nan'))):>12}"
              f" | drops {L(rs, lambda r: r['metrics']['msgs_dropped']):>12}"
              f" | qfull {L(rs, lambda r: r['metrics']['peers_send_queue_full']):>4}"
              f" | nDown {L(rs, lambda r: r['neighbor_down']):>6}"
              f" | rss {L(rs, lambda r: r['max_rss_mb']):>8}")
print("== fast")
for v in variants:
    rs = rows[("fast", v)]
    print(f"  {v:>13}: delivered {L(rs, lambda r: r['healthy_nodes']['delivered_mean'] * 100, '{:.0f}%'):>9}"
          f" | p50 ms {L(rs, lambda r: r['healthy_nodes']['age_p50_ms']):>8}"
          f" | p99 ms {L(rs, lambda r: r['healthy_nodes']['age_p99_ms']):>10}")
