#!/bin/bash
#
# Install Vagrant + libvirt on Ubuntu 24.04
# Usage: ./setup.sh [--install] [--reinstall current|latest] [--uninstall]
#
# Options:
#   --install              Install fixed versions (default behavior)
#   --reinstall current    Reinstall all components keeping current versions
#   --reinstall latest     Reinstall all components to latest versions
#   --uninstall            Completely uninstall all components
#
# Default behavior: Install fixed versions (pinned to specific versions)
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

# Fixed versions (pinned for reproducible installs)
FIXED_VAGRANT_VERSION="2.4.9"
FIXED_VAGRANT_LIBVIRT_PLUGIN_VERSION="0.9.0"

# Parse arguments
MODE="install"  # install, reinstall-current, reinstall-latest, uninstall

while [[ $# -gt 0 ]]; do
    case $1 in
        --install)
            MODE="install"
            shift
            ;;
        --reinstall)
            if [ $# -lt 2 ]; then
                log_error "--reinstall requires an argument: current or latest"
                exit 1
            fi
            case $2 in
                current)
                    MODE="reinstall-current"
                    shift 2
                    ;;
                latest)
                    MODE="reinstall-latest"
                    shift 2
                    ;;
                *)
                    log_error "Invalid --reinstall option: $2 (must be 'current' or 'latest')"
                    exit 1
                    ;;
            esac
            ;;
        --uninstall)
            MODE="uninstall"
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [--install] [--reinstall current|latest] [--uninstall]"
            echo ""
            echo "Options:"
            echo "  --install              Install fixed versions (default)"
            echo "  --reinstall current    Reinstall keeping current versions"
            echo "  --reinstall latest     Reinstall to latest versions"
            echo "  --uninstall            Completely uninstall all components"
            echo ""
            echo "Default: Install fixed versions (Vagrant ${FIXED_VAGRANT_VERSION}, plugin ${FIXED_VAGRANT_LIBVIRT_PLUGIN_VERSION})"
            exit 0
            ;;
        *)
            log_error "Unknown option: $1"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

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

# Handle uninstall mode first
if [ "$MODE" = "uninstall" ]; then
    log_warn "This will uninstall Vagrant, libvirt, and related components."
    log_warn "This will also remove any Vagrant VMs and the vagrant-libvirt network."
    read -p "Are you sure you want to continue? (y/N) " -n 1 -r
    echo
    [[ ! $REPLY =~ ^[Yy]$ ]] && exit 0
    
    echo ""
    
    # Remove vagrant-libvirt network
    if virsh net-info vagrant-libvirt &>/dev/null; then
        log_info "Removing vagrant-libvirt network..."
        virsh net-destroy vagrant-libvirt 2>/dev/null || true
        virsh net-undefine vagrant-libvirt 2>/dev/null || true
        log_info "Network removed"
    else
        log_info "vagrant-libvirt network not found (skipping)"
    fi
    
    # Remove vagrant-libvirt plugin
    if vagrant plugin list 2>/dev/null | grep -q vagrant-libvirt; then
        log_info "Uninstalling vagrant-libvirt plugin..."
        vagrant plugin uninstall vagrant-libvirt
        log_info "Plugin removed"
    else
        log_info "vagrant-libvirt plugin not found (skipping)"
    fi
    
    # Remove Vagrant
    if command -v vagrant &> /dev/null; then
        log_info "Uninstalling Vagrant..."
        sudo apt-get remove -y vagrant
        sudo apt-get autoremove -y
        log_info "Vagrant removed"
    else
        log_info "Vagrant not found (skipping)"
    fi
    
    # Note: We do NOT remove the HashiCorp repository as it may be used by other tools
    # (Terraform, Packer, Consul, etc.). Only Vagrant-specific components are removed.
    log_info "Keeping HashiCorp repository (may be used by other tools like Terraform, Packer, etc.)"
    
    # Note: We do NOT disable libvirtd as it may be used by other virtualization tools
    # Only Vagrant-specific components are removed.
    log_info "Keeping libvirtd service (may be used by other virtualization tools)"
    
    # Note: We do NOT remove user from libvirt/kvm groups as they may be needed
    # for other virtualization tools. Only Vagrant-specific components are removed.
    log_info "Keeping user in libvirt/kvm groups (may be needed for other tools)"
    
    # Note: We do NOT remove KVM/libvirt packages as they may be used by other tools
    # Only Vagrant-specific components are removed.
    log_info "Keeping KVM/libvirt packages (may be used by other virtualization tools)"
    
    # Clean up Vagrant directories
    log_info "Cleaning up Vagrant directories..."
    rm -rf ~/.vagrant.d
    rm -rf ~/.cache/vagrant
    log_info "Vagrant directories cleaned"
    
    echo ""
    log_info "════════════════════════════════════════════════════════════"
    log_info "  Uninstallation complete!"
    log_info "════════════════════════════════════════════════════════════"
    log_warn ""
    log_info ""
    log_info "  Uninstall removed only Vagrant-specific components:"
    log_info "    ✓ vagrant-libvirt network"
    log_info "    ✓ vagrant-libvirt plugin"
    log_info "    ✓ Vagrant package"
    log_info "    ✓ Vagrant user directories (~/.vagrant.d, ~/.cache/vagrant)"
    log_info ""
    log_info "  The following were kept (may be used by other tools):"
    log_info "    • HashiCorp repository (Terraform, Packer, etc.)"
    log_info "    • libvirtd service (other virtualization tools)"
    log_info "    • libvirt/kvm groups and packages"
    log_info ""
    exit 0
fi

# Installation modes
case $MODE in
    reinstall-latest)
        log_info "Reinstalling Vagrant + libvirt to latest versions for Ubuntu ${VERSION_ID}..."
        ;;
    reinstall-current)
        log_info "Reinstalling Vagrant + libvirt (keeping current versions) for Ubuntu ${VERSION_ID}..."
        ;;
    install|*)
        log_info "Installing Vagrant + libvirt (fixed versions) for Ubuntu ${VERSION_ID}..."
        log_info "  Vagrant: ${FIXED_VAGRANT_VERSION}"
        log_info "  Plugin: ${FIXED_VAGRANT_LIBVIRT_PLUGIN_VERSION}"
        ;;
esac
echo ""

# Track if anything was installed
INSTALLED_SOMETHING=false

# Step 1: Install/Update KVM/libvirt
if [ "$MODE" = "reinstall-latest" ] || [ "$MODE" = "reinstall-current" ]; then
    log_info "Updating KVM and libvirt packages..."
    sudo apt-get update
    if [ "$MODE" = "reinstall-latest" ]; then
        sudo apt-get upgrade -y \
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
    else
        sudo apt-get install --reinstall -y \
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
    fi
    INSTALLED_SOMETHING=true
elif command -v virsh &> /dev/null && systemctl is-active --quiet libvirtd; then
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

# Step 4: Install/Update Vagrant from HashiCorp
# Check if HashiCorp repository already exists (may be installed for other tools)
REPO_ADDED=false

if [ ! -f /usr/share/keyrings/hashicorp-archive-keyring.gpg ]; then
    log_info "Adding HashiCorp GPG key..."
    wget -O - https://apt.releases.hashicorp.com/gpg | sudo gpg --dearmor -o /usr/share/keyrings/hashicorp-archive-keyring.gpg
    REPO_ADDED=true
    INSTALLED_SOMETHING=true
else
    log_skip "HashiCorp GPG key"
fi

if [ ! -f /etc/apt/sources.list.d/hashicorp.list ]; then
    log_info "Adding HashiCorp repository..."
    echo "deb [signed-by=/usr/share/keyrings/hashicorp-archive-keyring.gpg] https://apt.releases.hashicorp.com $(lsb_release -cs) main" | sudo tee /etc/apt/sources.list.d/hashicorp.list > /dev/null
    REPO_ADDED=true
    INSTALLED_SOMETHING=true
else
    log_skip "HashiCorp repository"
fi

# Only update package list if we added the repository, or if we're in reinstall mode
if [ "$REPO_ADDED" = true ] || [ "$MODE" = "reinstall-latest" ] || [ "$MODE" = "reinstall-current" ]; then
    sudo apt-get update
fi

if [ "$MODE" = "reinstall-latest" ] || [ "$MODE" = "reinstall-current" ]; then
    if [ "$MODE" = "reinstall-latest" ]; then
        log_info "Updating Vagrant to latest version..."
        sudo apt-get upgrade -y vagrant
    else
        CURRENT_VERSION=$(vagrant --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' || echo "")
        if [ -n "$CURRENT_VERSION" ]; then
            log_info "Reinstalling Vagrant (keeping version ${CURRENT_VERSION})..."
            sudo apt-get install --reinstall -y "vagrant=${CURRENT_VERSION}" || sudo apt-get install --reinstall -y vagrant
        else
            log_info "Reinstalling Vagrant..."
            sudo apt-get install --reinstall -y vagrant
        fi
    fi
    INSTALLED_SOMETHING=true
elif command -v vagrant &> /dev/null; then
    log_skip "Vagrant ($(vagrant --version))"
else
    log_info "Installing Vagrant ${FIXED_VAGRANT_VERSION} from HashiCorp repository..."
    # Try to install specific version, fall back to latest if version not available
    sudo apt-get install -y "vagrant=${FIXED_VAGRANT_VERSION}" 2>/dev/null || {
        log_warn "Version ${FIXED_VAGRANT_VERSION} not available, installing latest..."
        sudo apt-get install -y vagrant
    }
    INSTALLED_SOMETHING=true
fi

# Step 5: Install/Update vagrant-libvirt plugin
if [ "$MODE" = "reinstall-latest" ] || [ "$MODE" = "reinstall-current" ]; then
    if vagrant plugin list 2>/dev/null | grep -q vagrant-libvirt; then
        if [ "$MODE" = "reinstall-latest" ]; then
            log_info "Updating vagrant-libvirt plugin to latest version..."
            vagrant plugin update vagrant-libvirt
        else
            CURRENT_PLUGIN_VERSION=$(vagrant plugin list 2>/dev/null | grep vagrant-libvirt | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || echo "")
            if [ -n "$CURRENT_PLUGIN_VERSION" ]; then
                log_info "Reinstalling vagrant-libvirt plugin (keeping version ${CURRENT_PLUGIN_VERSION})..."
                vagrant plugin uninstall vagrant-libvirt
                vagrant plugin install vagrant-libvirt --plugin-version "${CURRENT_PLUGIN_VERSION}"
            else
                log_info "Reinstalling vagrant-libvirt plugin..."
                vagrant plugin uninstall vagrant-libvirt
                vagrant plugin install vagrant-libvirt
            fi
        fi
    else
        log_info "Installing vagrant-libvirt plugin..."
        vagrant plugin install vagrant-libvirt
    fi
    INSTALLED_SOMETHING=true
elif vagrant plugin list 2>/dev/null | grep -q vagrant-libvirt; then
    INSTALLED_VERSION=$(vagrant plugin list 2>/dev/null | grep vagrant-libvirt | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || echo "")
    if [ -n "$INSTALLED_VERSION" ]; then
        log_skip "vagrant-libvirt plugin (version ${INSTALLED_VERSION})"
    else
        log_skip "vagrant-libvirt plugin"
    fi
else
    log_info "Installing vagrant-libvirt plugin ${FIXED_VAGRANT_LIBVIRT_PLUGIN_VERSION}..."
    # Try to install specific version, fall back to latest if version not available
    vagrant plugin install vagrant-libvirt --plugin-version "${FIXED_VAGRANT_LIBVIRT_PLUGIN_VERSION}" 2>/dev/null || {
        log_warn "Version ${FIXED_VAGRANT_LIBVIRT_PLUGIN_VERSION} not available, installing latest..."
        vagrant plugin install vagrant-libvirt
    }
    INSTALLED_SOMETHING=true
fi

# Step 6: Configure vagrant-libvirt network to autostart
if virsh net-info vagrant-libvirt &>/dev/null; then
    # Network exists, check if autostart is enabled
    if virsh net-info vagrant-libvirt 2>/dev/null | grep -q "Autostart:.*yes"; then
        log_skip "vagrant-libvirt network autostart"
    else
        log_info "Configuring vagrant-libvirt network to autostart..."
        virsh net-autostart vagrant-libvirt
        INSTALLED_SOMETHING=true
    fi
    
    # Start the network if it's not running
    if virsh net-info vagrant-libvirt 2>/dev/null | grep -q "Active:.*yes"; then
        log_skip "vagrant-libvirt network (already running)"
    else
        log_info "Starting vagrant-libvirt network..."
        virsh net-start vagrant-libvirt
        INSTALLED_SOMETHING=true
    fi
else
    log_info "vagrant-libvirt network doesn't exist yet (will be created on first 'vagrant up')"
fi

# Step 7: Verify installation
echo ""
log_info "Verifying installation..."
echo ""
echo "Versions installed:"
echo "  Vagrant: $(vagrant --version 2>/dev/null || echo 'not found')"
echo "  libvirt: $(virsh --version 2>/dev/null || echo 'not found')"
echo "  Plugins: $(vagrant plugin list 2>/dev/null | grep libvirt || echo 'vagrant-libvirt not found')"
if virsh net-info vagrant-libvirt &>/dev/null; then
    echo "  Network: vagrant-libvirt ($(virsh net-info vagrant-libvirt 2>/dev/null | grep -E 'Active|Autostart' | tr '\n' ' '))"
fi

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
log_info "  To manage components later:"
log_info "    ./setup.sh --install              # Install fixed versions"
log_info "    ./setup.sh --reinstall current   # Reinstall current versions"
log_info "    ./setup.sh --reinstall latest    # Reinstall to latest versions"
log_info "    ./setup.sh --uninstall           # Completely uninstall"
log_info ""