#!/usr/bin/env python3
"""Rewrite an APK so every entry is stored, then align and sign it.

Bevy 0.13 reads Android assets with AAsset_getBuffer, which fails when aapt
compresses the entry. cargo-apk compresses assets for release builds.
"""

import os
import shutil
import subprocess
import sys
import zipfile


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(f"usage: {sys.argv[0]} APK")
    src = os.path.abspath(sys.argv[1])
    sdk = os.environ["ANDROID_HOME"]
    keystore = os.environ["CARGO_APK_RELEASE_KEYSTORE"]
    password = os.environ["CARGO_APK_RELEASE_KEYSTORE_PASSWORD"]
    tools = os.path.join(sdk, "build-tools", "34.0.0")
    work = src + ".store-work"
    shutil.rmtree(work, ignore_errors=True)
    os.makedirs(work)
    with zipfile.ZipFile(src) as archive:
        archive.extractall(work)
    shutil.rmtree(os.path.join(work, "META-INF"), ignore_errors=True)
    unaligned = src + ".unaligned"
    with zipfile.ZipFile(unaligned, "w") as archive:
        for root, _, files in os.walk(work):
            for name in files:
                path = os.path.join(root, name)
                archive.write(
                    path,
                    os.path.relpath(path, work),
                    compress_type=zipfile.ZIP_STORED,
                )
    aligned = src + ".aligned"
    subprocess.check_call(
        [os.path.join(tools, "zipalign"), "-f", "-p", "4", unaligned, aligned]
    )
    subprocess.check_call(
        [
            os.path.join(tools, "apksigner"),
            "sign",
            "--ks",
            keystore,
            "--ks-pass",
            "pass:" + password,
            aligned,
        ]
    )
    os.replace(aligned, src)
    os.remove(unaligned)
    shutil.rmtree(work)


if __name__ == "__main__":
    main()
