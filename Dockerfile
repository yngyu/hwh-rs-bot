FROM rust:1.89 AS builder

ARG  WORKDIR="/usr/src/hwh-rs-bot"

WORKDIR ${WORKDIR}

COPY . .
RUN apt-get update && apt-get install -y --no-install-recommends \
    cmake=* \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

SHELL ["/bin/bash", "-o", "pipefail", "-c"]
RUN cargo install --path .


FROM debian:13.0-slim

ARG  USER_ID="10000"
ARG  GROUP_ID="10001"
ARG  USER_NAME="user"

COPY --from=builder /usr/local/cargo/bin/hwh-rs-bot /usr/local/bin/hwh-rs-bot

RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl-dev=* \
    ca-certificates=* \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd -g "${GROUP_ID}" "${USER_NAME}" && \
    useradd -l -u "${USER_ID}" -m "${USER_NAME}" -g "${USER_NAME}"

RUN chown -R ${USER_NAME} /usr/local/bin/hwh-rs-bot

USER ${USER_NAME}
CMD ["hwh-rs-bot"]
