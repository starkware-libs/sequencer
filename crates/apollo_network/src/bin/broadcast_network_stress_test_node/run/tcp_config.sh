#!/bin/bash
set -e

# Fixed window size: 64MB (67108864 bytes)
WINDOW_SIZE=67108864

# General core socket buffer limits
echo "net.core.rmem_max = 1073741824" > /etc/sysctl.conf
echo "net.core.wmem_max = 1073741824" >> /etc/sysctl.conf
echo "net.ipv4.tcp_rmem = 262144 1048576 1073741824" >> /etc/sysctl.conf
echo "net.ipv4.tcp_wmem = 262144 1048576 1073741824" >> /etc/sysctl.conf

# sysctl -p