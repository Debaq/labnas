import { test } from 'node:test'
import assert from 'node:assert/strict'
import { parseBom, shortFootprint, matches, splitCsv } from '../src/lib/bom.ts'

test('CSV de KiCad: agrupa por valor y huella', () => {
  const bom = parseBom(`"Reference","Value","Footprint","Qty"
"R1,R2","10k","Resistor_SMD:R_0805_2012Metric","2"
"R3","10k","Resistor_SMD:R_0805_2012Metric","1"
"C1","100nF","Capacitor_SMD:C_0603_1608Metric","1"
"U1","ATmega328P-AU","Package_QFP:TQFP-32_7x7mm_P0.8mm","1"`)
  assert.equal(bom.length, 3)
  assert.deepEqual(bom[0].refs, ['R1', 'R2', 'R3'])
  assert.equal(bom[0].qty, 3)
})

test('sin columna Qty cuenta las referencias; separador ;', () => {
  const bom = parseBom('Reference;Value;Footprint\nR1 R2 R3;1k;R_0603\n')
  assert.equal(bom[0].qty, 3)
})

test('huella corta y comillas', () => {
  assert.equal(shortFootprint('Resistor_SMD:R_0805_2012Metric'), '0805')
  assert.equal(shortFootprint('Package_DIP:DIP-8_W7.62mm'), 'DIP-8_W7.62mm')
  assert.deepEqual(splitCsv('"a, b","c ""d"""', ','), ['a, b', 'c "d"'])
})

test('cruce con el inventario', () => {
  const line = { refs: ['R1'], value: '10k', footprint: 'Resistor_SMD:R_0805_2012Metric', qty: 1 }
  assert.ok(matches(line, 'Resistencia 10k 0805 1%'))
  assert.ok(!matches(line, 'Resistencia 10k 0603'))
  assert.ok(!matches(line, 'Resistencia 100k 0805'))
})

test('sin columna Value avisa', () => {
  assert.throws(() => parseBom('Ref,Foo\nR1,x'))
})
