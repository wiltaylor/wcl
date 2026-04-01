#!/usr/bin/env bash
set -euo pipefail

# Demonstrate WCL transform features.

DIR="$(cd "$(dirname "$0")" && pwd)"
TRANSFORMS="$DIR/../transforms/transforms.wcl"
ORDER_TRANSFORMS="$DIR/../transforms/order_transforms.wcl"
EMPLOYEES="$DIR/../transforms/employees.json"
ORDERS="$DIR/../transforms/orders.json"

echo "=== 1. Rename fields ==="
echo "  Transform: rename first/last → full_name, age → years_old"
wcl transform run rename-fields -f "$TRANSFORMS" --input "$EMPLOYEES"
echo ""

echo "=== 2. Enrich with computed fields ==="
echo "  Transform: add age_months, is_senior, role from department"
wcl transform run enrich -f "$TRANSFORMS" --input "$EMPLOYEES"
echo ""

echo "=== 3. Filter: seniors only (age >= 30) ==="
wcl transform run seniors-only -f "$TRANSFORMS" --input "$EMPLOYEES"
echo ""

echo "=== 4. Multi-filter: senior engineers ==="
wcl transform run senior-engineers -f "$TRANSFORMS" --input "$EMPLOYEES"
echo ""

echo "=== 5. Contact info (select fields) ==="
wcl transform run contact-info -f "$TRANSFORMS" --input "$EMPLOYEES"
echo ""

echo "=== 6. Normalize strings (upper, lower, trim, replace) ==="
wcl transform run normalize -f "$TRANSFORMS" --input "$EMPLOYEES"
echo ""

echo "=== 7. Conditional logic (ternary, boolean) ==="
wcl transform run classify -f "$TRANSFORMS" --input "$EMPLOYEES"
echo ""

echo "=== 8. Shipped orders ==="
wcl transform run shipped-orders -f "$ORDER_TRANSFORMS" --input "$ORDERS"
echo ""

echo "=== 9. High-value orders (> \$200) ==="
wcl transform run high-value -f "$ORDER_TRANSFORMS" --input "$ORDERS"
echo ""

echo "=== 10. Per-item cost calculation ==="
wcl transform run per-item-cost -f "$ORDER_TRANSFORMS" --input "$ORDERS"
echo ""

echo "=== 11. Order summary with boolean computed field ==="
wcl transform run order-summary -f "$ORDER_TRANSFORMS" --input "$ORDERS"
echo ""

echo "=== 12. Binary → JSON (struct + layout) ==="
wcl transform run sensor-to-json -f "$DIR/../transforms/binary_format.wcl" --input "$DIR/../transforms/sensor_data.bin"
echo ""

echo "=== 13. Text (TSV) → JSON ==="
wcl transform run tsv-to-json -f "$DIR/../transforms/text_log.wcl" --input "$DIR/../transforms/access.log"
echo ""

echo "=== 14. Text (space-separated) → JSON ==="
wcl transform run space-log -f "$DIR/../transforms/text_log.wcl" --input "$DIR/../transforms/requests.log"
echo ""

echo "=== 15. ZIP metadata → JSON (binary struct parsing) ==="
wcl transform run zip-to-json -f "$DIR/../transforms/zip_metadata.wcl" --input "$DIR/../transforms/sample.zip"
echo ""

echo "=== All transform examples complete ==="
