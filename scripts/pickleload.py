import pickletools

with open("tasks-phase2-calibration-1790712326687-0.pkl", "rb") as f:
    data = f.read()

with open("pickle_contents.txt", "w", encoding="utf-8") as out:
    pickletools.dis(data, out=out)