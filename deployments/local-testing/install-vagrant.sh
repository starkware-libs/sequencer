#!/bin/bash
#
# Install Vagrant + libvirt on Ubuntu 24.04
# Usage: ./install-vagrant.sh
#
# This script is idempotent - safe to run multiple times.
#

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info() { echo -e "${GREEN}[INFO]${NC} $*"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*"; }
log_skip() { echo -e "${YELLOW}[SKIP]${NC} $* (already installed)"; }

# Check if running as root
if [ "$EUID" -eq 0 ]; then
    log_error "Do not run this script as root. Run as your normal user."
    log_error "The script will use sudo when needed."
    exit 1
fi

# Detect OS
if [ ! -f /etc/os-release ]; then
    log_error "Cannot detect OS. This script is for Ubuntu 24.04."
    exit 1
fi

source /etc/os-release
if [ "$ID" != "ubuntu" ]; then
    log_warn "This script is designed for Ubuntu. Detected: $ID"
    read -p "Continue anyway? (y/N) " -n 1 -r
    echo
    [[ ! $REPLY =~ ^[Yy]$ ]] && exit 1
fi

log_info "Checking/Installing Vagrant + libvirt for Ubuntu ${VERSION_ID}..."
echo ""

# Track if anything was installed
INSTALLED_SOMETHING=false

# Step 1: Install KVM/libvirt
if command -v virsh &> /dev/null && systemctl is-active --quiet libvirtd; then
    log_skip "KVM/libvirt"
else
    log_info "Installing KVM and libvirt..."
    sudo apt-get update
    sudo apt-get install -y \
        qemu-kvm \
        libvirt-daemon-system \
        libvirt-clients \
        libguestfs-tools \
        bridge-utils \
        virtinst \
        libvirt-dev \
        ruby-dev \
        ruby-libvirt \
        ebtables \
        dnsmasq-base \
        rsync
    INSTALLED_SOMETHING=true
fi

# Step 2: Add user to groups (idempotent - no harm if already in group)
if groups "$USER" | grep -q '\blibvirt\b'; then
    log_skip "User $USER already in libvirt group"
else
    log_info "Adding $USER to libvirt and kvm groups..."
    sudo usermod -aG libvirt "$USER"
    sudo usermod -aG kvm "$USER"
    INSTALLED_SOMETHING=true
fi

# Step 3: Enable libvirtd
if systemctl is-enabled --quiet libvirtd 2>/dev/null; then
    log_skip "libvirtd service already enabled"
else
    log_info "Enabling libvirtd service..."
    sudo systemctl enable --now libvirtd
    INSTALLED_SOMETHING=true
fi

# Step 4: Install Vagrant from HashiCorp
if command -v vagrant &> /dev/null; then
    log_skip "Vagrant ($(vagrant --version))"
else
    log_info "Installing Vagrant from HashiCorp repository..."
    
    if [ ! -f /usr/share/keyrings/hashicorp-archive-keyring.gpg ]; then
        wget -O - https://apt.releases.hashicorp.com/gpg | sudo gpg --dearmor -o /usr/share/keyrings/hashicorp-archive-keyring.gpg
    fi
    
    echo "deb [signed-by=/usr/share/keyrings/hashicorp-archive-keyring.gpg] https://apt.releases.hashicorp.com $(lsb_release -cs) main" | sudo tee /etc/apt/sources.list.d/hashicorp.list > /dev/null
    
    sudo apt-get update
    sudo apt-get install -y vagrant
    INSTALLED_SOMETHING=true
fi

# Step 5: Install vagrant-libvirt plugin
if vagrant plugin list 2>/dev/null | grep -q vagrant-libvirt; then
    log_skip "vagrant-libvirt plugin"
else
    log_info "Installing vagrant-libvirt plugin..."
    vagrant plugin install vagrant-libvirt
    INSTALLED_SOMETHING=true
fi

# Step 6: Verify installation
echo ""
log_info "Verifying installation..."
echo ""
echo "Versions installed:"
echo "  Vagrant: $(vagrant --version 2>/dev/null || echo 'not found')"
echo "  libvirt: $(virsh --version 2>/dev/null || echo 'not found')"
echo "  Plugins: $(vagrant plugin list 2>/dev/null | grep libvirt || echo 'vagrant-libvirt not found')"

# Done
echo ""
log_info "════════════════════════════════════════════════════════════"

if [ "$INSTALLED_SOMETHING" = true ]; then
    log_info "  Installation complete!"
    log_info "════════════════════════════════════════════════════════════"
    log_warn ""
    log_warn "  IMPORTANT: Log out and log back in for group changes to take effect."
    log_warn "  Or run: newgrp libvirt"
    log_warn ""
else
    log_info "  Everything already installed! ✓"
    log_info "════════════════════════════════════════════════════════════"
    echo ""
fi

log_info "  To start the VM, run:"
log_info "    cd deployments/local-testing"
log_info "    vagrant up"
log_info ""
