#!/usr/bin/env sh
# rhwp 리눅스 실행 파일을 Docker 로 만든다. 저장소 루트에서 실행한다.
#   integrations/rhwp-was/linux/build-linux.sh            # glibc (Debian 12)
#   TARGET=x86_64-unknown-linux-musl integrations/rhwp-was/linux/build-linux.sh   # 정적
#   BASE=rust:1.93.1-bullseye integrations/rhwp-was/linux/build-linux.sh          # glibc 2.31
set -eu
cd "$(dirname "$0")/../../.."
OUT=integrations/rhwp-was/linux/out
docker build -f integrations/rhwp-was/linux/Dockerfile --target export -o "$OUT" \
  --build-arg "BASE=${BASE:-rust:1.93.1-bookworm}" --build-arg "TARGET=${TARGET:-x86_64-unknown-linux-gnu}" .
ls -la "$OUT/rhwp"
sha256sum "$OUT/rhwp" | tee "$OUT/rhwp.sha256"
echo "서버에서 확인: ldd $OUT/rhwp (musl 이면 'not a dynamic executable'), 그리고 echo '{\"ping\":true}' | ./rhwp worker"
