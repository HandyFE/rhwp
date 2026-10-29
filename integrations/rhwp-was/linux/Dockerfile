# rhwp 리눅스 실행 파일 빌드 (rhwp converter 워커용).
#
# 저장소 루트에서:
#   docker build -f integrations/rhwp-was/linux/Dockerfile --target export -o integrations/rhwp-was/linux/out .
# → integrations/rhwp-was/linux/out/rhwp (x86_64, glibc 2.36+ / Debian 12 계열).
#
# 더 오래된 glibc(RHEL 8 = 2.28, RHEL 7 = 2.17)의 서버에는 --build-arg BASE=rust:1.93.1-bullseye(2.31)
# 처럼 더 낮은 베이스를 쓰거나, --build-arg TARGET=x86_64-unknown-linux-musl 로 정적 링크한다
# (musl 은 메모리 할당기 성능이 떨어지므로 워커 처리 시간을 다시 실측한다).
ARG BASE=rust:1.93.1-bookworm
FROM ${BASE} AS builder
ARG TARGET=x86_64-unknown-linux-gnu
RUN if [ "$TARGET" = "x86_64-unknown-linux-musl" ]; then \
      apt-get update && apt-get install -y --no-install-recommends musl-tools && rustup target add "$TARGET"; \
    fi
WORKDIR /src
COPY . .
# CI 와 같은 잠금 빌드. 워커는 `rhwp` 바이너리만 쓴다.
RUN cargo build --locked --release --bin rhwp --target "$TARGET" --target-dir /build \
 && install -D "/build/$TARGET/release/rhwp" /out/rhwp \
 && /out/rhwp --version \
 && echo '{"ping":true}' | /out/rhwp worker

FROM scratch AS export
COPY --from=builder /out/rhwp /rhwp
