#!/usr/bin/env bash
# Reproducible Pipeline: Pop!_OS 24.04 with COSMIC + RustDesk
# Uses ISO install for proper COSMIC desktop

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOG_FILE="/tmp/popos-cosmic-build-$(date +%Y%m%d-%H%M%S).log"

echo "╔══════════════════════════════════════════════════════════════════════════╗"
echo "║  Pop!_OS 24.04 COSMIC + RustDesk Pipeline                               ║"
echo "╚══════════════════════════════════════════════════════════════════════════╝"
echo ""
echo "This pipeline creates a Pop!_OS 24.04 VM with:"
echo "  - COSMIC desktop (built-in to Pop!_OS 24.04)"
echo "  - RustDesk for remote access"
echo "  - Auto-configured for testing"
echo ""
echo "Log: ${LOG_FILE}"
echo ""

# Check for ISO
ISO_FILE="../agentReagents/images/iso/pop-os_24.04_amd64_intel.iso"

if [ ! -f "${ISO_FILE}" ]; then
    echo "❌ Pop!_OS ISO not found"
    echo "   Expected: ${ISO_FILE}"
    echo ""
    echo "Download it first:"
    echo "  cd ../agentReagents/scripts"
    echo "  ./download-pop-os-24.sh"
    exit 1
fi

echo "✅ ISO found: ${ISO_FILE}"
ISO_SIZE=$(du -h "${ISO_FILE}" | cut -f1)
echo "   Size: ${ISO_SIZE}"
echo ""

# Create VM from ISO
VM_NAME="popos-cosmic-rustdesk-$(date +%Y%m%d-%H%M%S)"

echo "╔══════════════════════════════════════════════════════════════════════════╗"
echo "║  Step 1: Creating Pop!_OS VM from ISO                                   ║"
echo "╚══════════════════════════════════════════════════════════════════════════╝"
echo ""
echo "VM Name: ${VM_NAME}"
echo "Memory: 4GB"
echo "vCPUs: 2"
echo "Disk: 50GB"
echo ""
echo "⚠️  MANUAL INSTALLATION REQUIRED"
echo ""
echo "Creating VM and starting installer..."
echo ""

# Create disk
DISK_PATH="/var/lib/libvirt/images/${VM_NAME}.qcow2"
sudo qemu-img create -f qcow2 "${DISK_PATH}" 50G

# Create VM
sudo virt-install \
    --name="${VM_NAME}" \
    --memory=4096 \
    --vcpus=2 \
    --disk path="${DISK_PATH}",format=qcow2,bus=virtio \
    --cdrom="${ISO_FILE}" \
    --os-variant=ubuntu24.04 \
    --network network=default,model=virtio \
    --graphics vnc,listen=0.0.0.0 \
    --noautoconsole \
    --boot cdrom,hd

# Get VNC display
sleep 3
VNC_DISPLAY=$(sudo virsh vncdisplay "${VM_NAME}")
VNC_PORT=$((5900 + ${VNC_DISPLAY#:}))

echo ""
echo "╔══════════════════════════════════════════════════════════════════════════╗"
echo "║  VM Created - Manual Installation Required                              ║"
echo "╚══════════════════════════════════════════════════════════════════════════╝"
echo ""
echo "VM: ${VM_NAME}"
echo "VNC: localhost:${VNC_PORT}"
echo ""
echo "Installation steps:"
echo "  1. Connect via VNC: vncviewer localhost:${VNC_PORT}"
echo "  2. Follow Pop!_OS installer:"
echo "     - Language: English"
echo "     - Keyboard: US"
echo "     - Clean Install"
echo "     - Username: cosmic"
echo "     - Password: cosmic2025"
echo "     - Hostname: popos-cosmic"
echo "  3. Let installer complete (~15 minutes)"
echo "  4. Reboot when prompted"
echo "  5. Run post-install script:"
echo "     ./popos-cosmic-post-install.sh ${VM_NAME}"
echo ""
echo "✅ VM ready for installation"
echo ""

# Save VM info
cat > "/tmp/${VM_NAME}-info.txt" << EOF
VM Name: ${VM_NAME}
VNC: localhost:${VNC_PORT}
IP: (will be assigned by DHCP after install)
Username: cosmic
Password: cosmic2025

Status: Waiting for manual installation

Next: ./popos-cosmic-post-install.sh ${VM_NAME}
EOF

echo "VM info saved: /tmp/${VM_NAME}-info.txt"
echo ""

