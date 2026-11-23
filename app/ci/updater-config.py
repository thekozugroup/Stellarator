"""Turns the nsh app's in-app updates on for a build: adds nsh.json's "updater" section with the
release key's public key (the environment's PUBLIC_KEY, the contents of `nsh keygen`'s .pub file)
and the manifest of the repository's latest release, nsh-latest.json. `nsh package` then ships it
as assets/nsh-updater.json, which app/src/updater.rs reads.

    PUBLIC_KEY=... python3 app/ci/updater-config.py app/nsh.json owner/repo
"""

import json
import os
import sys


def main(path: str, repository: str) -> None:
    key = os.environ.get("PUBLIC_KEY", "").strip()
    if not key:
        sys.exit("PUBLIC_KEY is empty")
    config = json.load(open(path))
    config["updater"] = {
        "url": f"https://github.com/{repository}/releases/latest/download/nsh-latest.json",
        "publicKey": key,
    }
    json.dump(config, open(path, "w"), indent=2)
    print(f"updater: {config['updater']['url']}")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
