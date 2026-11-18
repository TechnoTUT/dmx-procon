#!/bin/bash
# Linux 用の配る形(tar.gz)を作る。中身は実行ファイル・udev のルール・デスクトップのファイル
#   ./scripts/package-linux.sh x86_64|arm64   (先に cargo build --release しておく)
set -euo pipefail
cd "$(dirname "$0")/.."
ARCH="${1:?x86_64 か arm64 を指定してください}"
STAGE=dist/linux/procon-light
rm -rf "$STAGE" && mkdir -p "$STAGE"
cp target/release/procon-light "$STAGE/"
cp scripts/linux/70-procon-light.rules scripts/linux/procon-light.desktop "$STAGE/"
cat > "$STAGE/README.txt" <<'TXT'
procon-light(Linux)

  ./procon-light でウィンドウが開きます。

Switch 2 Pro Controller・Pro Controller を管理者の権限なしで使えるよう、最初に1回だけ:
  sudo cp 70-procon-light.rules /etc/udev/rules.d/
  sudo udevadm control --reload && sudo udevadm trigger
そのあと、コントローラーを抜き差ししてください。

メニューに出すには、procon-light を ~/.local/bin に、procon-light.desktop を ~/.local/share/applications に置きます。
TXT
(cd dist/linux && tar czf "procon-light-linux-$ARCH.tar.gz" procon-light)
echo "Built: dist/linux/procon-light-linux-$ARCH.tar.gz"
