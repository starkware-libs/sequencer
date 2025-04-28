FROM ubuntu:24.04

ENV DEBIAN_FRONTEND="noninteractive"
RUN apt update
RUN apt install -y jq moreutils

ENTRYPOINT [ "/bin/bash", "-c" ]
