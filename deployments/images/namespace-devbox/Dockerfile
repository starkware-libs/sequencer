FROM base AS runtime

RUN /usr/sbin/adduser --disabled-password --gecos "" --uid 1001 devbox \
    && /usr/sbin/groupadd --gid 1002 docker \
    && /usr/sbin/groupadd kvm \
    && /usr/sbin/usermod -aG sudo devbox \
    && /usr/sbin/usermod -aG docker devbox

RUN echo "%sudo   ALL=(ALL:ALL) NOPASSWD:ALL" > /etc/sudoers \
    && echo "%devbox ALL=(ALL:ALL) NOPASSWD:ALL" >> /etc/sudoers \
    && echo "root ALL=(ALL:ALL) ALL" >> /etc/sudoers \
    && echo "Defaults env_keep += \"DEBIAN_FRONTEND\"" >> /etc/sudoers

RUN mkdir -p /opt/sequencer_venv && chown -R devbox:devbox /opt/sequencer_venv /var/tmp/rust

LABEL "namespace.devbox.metadata"='{"remote_user": "devbox", "whole_system_persistency": true, "setup_docker_client": true, "docker_sock_user": {"uid": 1001}}'

USER devbox
