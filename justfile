set dotenv-load

default:
    just --list

install:
    cargo install --path .

pstree cmd='runall':
    #!/usr/bin/env python3
    import os
    import subprocess as sp
    pids = sp.check_output(["pgrep", "{{cmd}}"]).decode("utf-8").strip()
    pids = pids.splitlines()
    for pid in pids:
        os.system(f"pstree -a -l {pid}")
