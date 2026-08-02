#!/usr/bin/env python3
"""Diff two extracted wskill graphs into an audit model."""
import json
import sys
from collections import Counter

HUB_IN = 6          # in-edges at which a unit reads as a hub
CAP = 5             # linking_discipline's related cap for content units

before = json.load(open(sys.argv[1]))
after = json.load(open(sys.argv[2]))


def model(g):
    units = {u["id"]: u for u in g["units"]}
    idx = {i["id"]: i for i in g["indexes"]}
    edges = {(u["id"], r) for u in g["units"] for r in u["related"]}
    pins = {p for i in g["indexes"] for p in i["pinned"]}
    indeg = Counter(t for _, t in edges)
    return units, idx, edges, pins, indeg


bu, bi, be, bp, bin_ = model(before)
au, ai, ae, ap, ain = model(after)

added_units = [au[i] for i in au.keys() - bu.keys()]
removed_units = [bu[i] for i in bu.keys() - au.keys()]
added_edges = sorted(ae - be)
removed_edges = sorted(be - ae)

# units whose in-degree crossed the hub threshold
new_hubs = [{"id": i, "before": bin_.get(i, 0), "after": ain[i]}
            for i in au if ain[i] >= HUB_IN and bin_.get(i, 0) < HUB_IN]
# units that went over the related cap
new_over_cap = [{"id": i, "before": len(bu[i]["related"]) if i in bu else 0,
                 "after": len(au[i]["related"])}
                for i in au if len(au[i]["related"]) > CAP
                and (i not in bu or len(bu[i]["related"]) <= CAP)]
# unpinned + unreferenced = orphan
orphans = [au[i] for i in au
           if i not in ap and ain.get(i, 0) == 0]
new_orphans = [u for u in orphans if u["id"] not in bu or
               (u["id"] in bp or bin_.get(u["id"], 0) > 0)]
# dangling related ids
known = set(au) | set(ai)
dangling = [{"from": s, "to": t} for s, t in ae if t not in known]
# edges with no "why" anywhere (proxy: target id/name not named in source prose)
bare = []
for s, t in ae:
    su = au.get(s)
    if not su or t not in au:
        continue
    tgt = au[t]
    named = t in su["summary"] or tgt["name"].lower() in su["summary"].lower()
    if not named:
        bare.append({"from": s, "to": t})


def health_of(units, idx, edges, pins, indeg):
    known = set(units) | set(idx)
    bare = 0
    for src, tgt in edges:
        su, tu = units.get(src), units.get(tgt)
        if not su or not tu:
            continue
        if not (tgt in su["summary"] or tu["name"].lower() in su["summary"].lower()):
            bare += 1
    return {
        "units": len(units), "edges": len(edges),
        "edges_per_unit": round(len(edges) / max(len(units), 1), 2),
        "orphans": sum(1 for i in units if i not in pins and indeg.get(i, 0) == 0),
        "hubs": sum(1 for i in units if indeg.get(i, 0) >= HUB_IN),
        "over_cap": sum(1 for u in units.values() if len(u["related"]) > CAP),
        "bare_edges": bare,
        "dangling": sum(1 for _, t in edges if t not in known),
        "indexes": len(idx),
        "bodyless_indexes": sum(1 for x in idx.values() if not x["has_body"]),
    }


hb = health_of(bu, bi, be, bp, bin_)
ha = health_of(au, ai, ae, ap, ain)
LABELS = [
    ("units", "Units", False), ("edges", "Related edges", False),
    ("edges_per_unit", "Edges per unit", True),
    ("orphans", "Unpinned + unreferenced", True),
    ("hubs", "Hub-shaped units (>=6 in)", True),
    ("over_cap", "Units over the 5-link cap", True),
    ("bare_edges", "Edges with no reason in prose", True),
    ("dangling", "Dangling related ids", True),
    ("indexes", "Indexes", False),
    ("bodyless_indexes", "Body-less indexes (unlinkable)", True),
]
metrics = []
for key, label, good_down in LABELS:
    d = round(ha[key] - hb[key], 2)
    verdict = "—" if d == 0 else (
        ("worse" if d > 0 else "better") if good_down else "grew" if d > 0 else "shrank")
    metrics.append({"label": label, "before": hb[key], "after": ha[key],
                    "delta": d, "good_down": good_down, "verdict": verdict})

out = {
    "health": {"metrics": metrics,
               "worse": sum(1 for m in metrics if m["verdict"] == "worse")},
    "before_rev": before["rev"], "after_rev": after["rev"],
    "counts": {
        "units_before": len(bu), "units_after": len(au),
        "edges_before": len(be), "edges_after": len(ae),
        "added_units": len(added_units), "removed_units": len(removed_units),
        "added_edges": len(added_edges), "removed_edges": len(removed_edges),
        "new_hubs": len(new_hubs), "new_over_cap": len(new_over_cap),
        "orphans": len(orphans), "dangling": len(dangling),
        "bare_edges": len(bare),
    },
    "added_units": sorted(added_units, key=lambda u: u["id"]),
    "removed_units": sorted(removed_units, key=lambda u: u["id"]),
    "added_edges": added_edges, "removed_edges": removed_edges,
    "new_hubs": new_hubs, "new_over_cap": new_over_cap,
    "orphans": sorted(orphans, key=lambda u: u["id"]),
    "dangling": dangling,
    "units_after": au, "indexes_after": ai,
    "units_before": bu,
    "edges_before": sorted(be), "edges_after": sorted(ae),
    "over_cap_after": {i: len(u["related"]) for i, u in au.items()
                       if len(u["related"]) > CAP},
    "pins_after": sorted(ap),
    "indeg_after": ain,
}
json.dump(out, sys.stdout, indent=1)
