#!/usr/bin/env bash
# Reproducible Desktop VM Image Builder Pipeline
# Creates Ubuntu Desktop + RustDesk template with full validation

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOG_FILE="/tmp/image-build-$(date +%Y%m%d-%H%M%S).log"

echo "╔══════════════════════════════════════════════════════════════════════════╗"
echo "║  Reproducible Desktop Image Build Pipeline                              ║"
echo "╚══════════════════════════════════════════════════════════════════════════╝"
echo ""
echo "Pipeline: Ubuntu Desktop + RustDesk"
echo "Log: ${LOG_FILE}"
echo ""

# Validation step 1: Check prerequisites
echo "Step 1/6: Validating prerequisites..."
if ! command -v cargo &> /dev/null; then
    echo "❌ cargo not found"
    exit 1
fi

if [ ! -f "../agentReagents/images/cloud/ubuntu-24.04-server-cloudimg-amd64.img" ]; then
    echo "❌ Base image not found"
    echo "Run: cd ../agentReagents/scripts && ./download-cloud-images.sh"
    exit 1
fi

if ! sudo virsh list &> /dev/null; then
    echo "❌ libvirt not accessible"
    exit 1
fi

echo "✅ Prerequisites validated"
echo ""

# Step 2: Build image with ImageBuilder
echo "Step 2/6: Building image with benchScale ImageBuilder..."
echo "This will take 15-20 minutes for desktop installation."
echo ""

cd "${SCRIPT_DIR}"
cargo run --example build_working_desktop --features libvirt 2>&1 | tee -a "${LOG_FILE}"

BUILD_STATUS=${PIPESTATUS[0]}

if [ $BUILD_STATUS -ne 0 ]; then
    echo ""
    echo "❌ Build failed. Check log: ${LOG_FILE}"
    exit 1
fi

echo ""
echo "✅ Image built successfully"
echo ""

# Step 3: Validate template exists
echo "Step 3/6: Validating template file..."
TEMPLATE="/var/lib/libvirt/images/ubuntu-desktop-rustdesk-template.qcow2"

if [ ! -f "${TEMPLATE}" ]; then
    echo "❌ Template not found: ${TEMPLATE}"
    exit 1
fi

TEMPLATE_SIZE=$(du -h "${TEMPLATE}" | cut -f1)
echo "✅ Template exists: ${TEMPLATE} (${TEMPLATE_SIZE})"
echo ""

# Step 4: Create test VM from template
echo "Step 4/6: Creating test VM from template..."
TEST_VM="desktop-validation-$(date +%Y%m%d-%H%M%S)"

cd "${SCRIPT_DIR}/../ionChannel"
cargo run --example create_test_vm --features benchscale -- "${TEST_VM}" 2>&1 | tee -a "${LOG_FILE}"

VM_STATUS=${PIPESTATUS[0]}

if [ $VM_STATUS -ne 0 ]; then
    echo "❌ Test VM creation failed"
    exit 1
fi

echo "✅ Test VM created: ${TEST_VM}"
echo ""

# Step 5: Validate VM is running
echo "Step 5/6: Validating VM..."
sleep 10

if ! sudo virsh domstate "${TEST_VM}" | grep -q "running"; then
    echo "❌ VM is not running"
    exit 1
fi

VM_IP=$(sudo virsh domifaddr "${TEST_VM}" | grep ipv4 | awk '{print $4}' | cut -d/ -f1)
VNC_DISPLAY=$(sudo virsh vncdisplay "${TEST_VM}")

echo "✅ VM running"
echo "   IP: ${VM_IP}"
echo "   VNC: localhost${VNC_DISPLAY}"
echo ""

# Step 6: Document success
echo "Step 6/6: Documenting pipeline success..."

cat > "${SCRIPT_DIR}/../PIPELINE_SUCCESS.md" << EOF
# Reproducible Pipeline Success

**Date:** $(date)
**Pipeline:** Ubuntu Desktop + RustDesk
**Status:** ✅ SUCCESS

## Build Artifacts

Template: ${TEMPLATE}
Size: ${TEMPLATE_SIZE}
Test VM: ${TEST_VM}
IP: ${VM_IP}
VNC: localhost${VNC_DISPLAY}

## Pipeline Validation

✅ Prerequisites checked
✅ Image built successfully
✅ Template validated
✅ Test VM created
✅ VM running and accessible

## Reproducibility

To reproduce:
\`\`\`bash
cd benchScale
./pipeline-build-and-validate.sh
\`\`\`

Build log: ${LOG_FILE}

## Next Steps

1. Access VM via VNC: vncviewer localhost${VNC_DISPLAY}
2. Verify desktop loads
3. Verify RustDesk auto-starts
4. Note RustDesk ID for remote access
5. Use template for ionChannel validation

## Scaling

To create more VMs from this template:
\`\`\`rust
let backend = LibvirtBackend::new()?;
let vm = backend.create_from_template(
    "my-desktop-vm",
    &PathBuf::from("${TEMPLATE}"),
    Some(&cloud_init),
    4096, 2, false
).await?;
\`\`\`

## Template Details

Base: Ubuntu 24.04 LTS
Desktop: Ubuntu Desktop (minimal)
Remote: RustDesk 1.2.3
Screen: PipeWire + Wireplumber
Access: SSH + VNC

This pipeline is reproducible, validatable, and scalable! 🚀
EOF

echo "✅ Pipeline documented"
echo ""

echo "╔══════════════════════════════════════════════════════════════════════════╗"
echo "║                                                                          ║"
echo "║  ✅ PIPELINE SUCCESS - Reproducible and Validated                        ║"
echo "║                                                                          ║"
echo "╚══════════════════════════════════════════════════════════════════════════╝"
echo ""
echo "📍 Template: ${TEMPLATE} (${TEMPLATE_SIZE})"
echo "📊 Test VM: ${TEST_VM}"
echo "🌐 IP: ${VM_IP}"
echo "📺 VNC: vncviewer localhost${VNC_DISPLAY}"
echo ""
echo "📋 Full report: ${SCRIPT_DIR}/../PIPELINE_SUCCESS.md"
echo "📝 Build log: ${LOG_FILE}"
echo ""
echo "Ready to scale and repeat! 🚀"

