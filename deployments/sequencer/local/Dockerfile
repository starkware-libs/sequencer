# Dockerfile for CDK8S Deployment Setup
# This Dockerfile sets up a container with all dependencies needed for the CDK8S deployment project
# Base image: Ubuntu 24.04 (also supports 22.04)
FROM ubuntu:24.04

# Set environment variables for non-interactive installation
ENV DEBIAN_FRONTEND=noninteractive \
    TZ=UTC

# Set working directory to deployments/sequencer
WORKDIR /workspace/deployments/sequencer

# Copy and run dependencies installation script
COPY local/dependencies.sh /tmp/dependencies.sh
RUN chmod +x /tmp/dependencies.sh && \
    /tmp/dependencies.sh

# Remove the default ubuntu user so UID 1000 is free for the host user
RUN if id -u ubuntu &>/dev/null; then \
        userdel -r ubuntu 2>/dev/null || true; \
    fi

# Copy .bashrc and .bash_profile templates to /etc/skel for new users
COPY local/.bashrc /etc/skel/.bashrc
COPY local/.bash_profile /etc/skel/.bash_profile

# Copy setup script to container
COPY local/setup.sh /usr/local/bin/setup.sh
RUN chmod +x /usr/local/bin/setup.sh

# Copy and set up entrypoint script to create user entry at runtime
COPY local/docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh

ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
CMD ["/bin/bash", "-l"]
