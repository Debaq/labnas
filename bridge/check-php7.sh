#!/usr/bin/env bash
# Revisa que el conector no use sintaxis o funciones posteriores a PHP 7.0.
# (Complementa al CI, que ademas ejecuta los tests con PHP 7.0, 7.4, 8.1 y 8.3.)
set -euo pipefail
cd "$(dirname "$0")"

files=$(find . -name '*.php' -not -path './web/*')
fail=0
check() {
    local desc=$1 pattern=$2
    # shellcheck disable=SC2086
    if grep -nE "$pattern" $files; then
        echo "  ^ no compatible con PHP 7.0: $desc"
        fail=1
    fi
}

check "funcion flecha (7.4)" '\bfn[[:space:]]*\('
check "operador ??= (7.4)" '\?\?='
check "nullsafe ?-> (8.0)" '\?->'
check "match (8.0)" '\bmatch[[:space:]]*\('
check "tipo nulable (7.1)" '(\(|,)[[:space:]]*\?[A-Za-z_]+[[:space:]]+\$|\)[[:space:]]*:[[:space:]]*\?[A-Za-z]'
check "retorno void (7.1)" '\)[[:space:]]*:[[:space:]]*void'
check "constante con visibilidad (7.1)" '(private|protected|public)[[:space:]]+const'
check "multi-catch (7.1)" 'catch[[:space:]]*\([^)]*\|'
check "destructuring con [] (7.1)" '^[[:space:]]*\[[^]]*\][[:space:]]*=[^=>]'
check "str_contains/str_starts_with/str_ends_with (8.0)" '\bstr_(contains|starts_with|ends_with)\('
check "array_key_first/last (7.3)" '\barray_key_(first|last)\('
check "atributos #[ (8.0)" '#\['
check "readonly/enum (8.1)" '\b(readonly|enum)[[:space:]]+[a-zA-Z]'
check "sodium (7.2)" '\bsodium_'
check "propiedad con tipo (7.4)" '(private|protected|public)[[:space:]]+[a-zA-Z?]+[[:space:]]+\$'

if [ "$fail" = 0 ]; then
    echo "OK: sin construcciones posteriores a PHP 7.0"
fi
exit $fail
