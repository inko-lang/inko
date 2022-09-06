import sys
import json


def version_sort(version):
    if "." in version:
        return version.split(".")
    else:
        return [version]


versions = []

for line in sys.stdin:
    chunks = line.strip()[1:].split(".")
    version = (int(chunks[0]), int(chunks[1]), int(chunks[2]))

    # We only started tracking the documentation per version as of version
    # 0.10.0, so we ignore any versions that came before it.
    if version[0] >= 0 and version[1] >= 10 and version[2] >= 0:
        versions.append(version)

versions = sorted(versions, reverse=True)
latest = None
entries = [
    {"version": "latest", "title": "latest", "aliases": []},
    {"version": "master", "title": "master", "aliases": []},
]

if versions:
    latest = versions[0]

for version in versions:
    aliases = []

    if version == latest:
        aliases.append("latest")

    name = f"v{version[0]}.{version[1]}.{version[2]}"

    entries.append({"version": name, "title": name, "aliases": aliases})

print(json.dumps(entries))
