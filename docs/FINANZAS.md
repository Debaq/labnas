# Finanzas

Etapa 1 de la [hoja de ruta](HOJA_DE_RUTA.md): que se pueda **demostrar** cómo se mueve el
dinero de cada proyecto. Módulo `finanzas` (Configuración > Módulos), página **Finanzas**.

## Base común

- **Un proyecto es una entrada del Portafolio.** Finanzas le agrega presupuesto, compras,
  viajes y rendición; no crea proyectos aparte.
- **Todo lo financiero va a la [bitácora inalterable](BITACORA.md)** (`finance::ledger`):
  aprobaciones, pagos, recepciones, cambios de presupuesto. Nada se edita ni se borra; un
  error se corrige con un registro nuevo.
- **Lo comprometido y lo gastado** de cada flujo (compra, viaje, caja chica, honorario) se
  anota en `fin_commitments` (`finance::set_commitment`). La ejecución presupuestaria suma
  esa tabla, así cada flujo nuevo cuenta sin tocar el presupuesto.
- **Respaldos** (facturas, cotizaciones, actas, fotos) en `fin_files` con su huella SHA-256,
  que también queda en la bitácora. PDF, XML, JPEG, PNG, XLSX o DOCX, hasta 15 MB.
- **Separación de funciones:** quien pide, quien aprueba y quien recibe son personas
  distintas cuando hay dinero de por medio.

### Niveles (plantillas de rol)

| Rol | Finanzas | Transparencia |
|---|---|---|
| Investigador responsable, administrativo | gestionar | gestionar |
| Docente | usar (pide compras) | ver |
| Operador, auditor | ver | ver |
| Resto | sin acceso | ver |

## Presupuesto y ejecución

*(ideas 52, 59, 60, 64, 65)*

## Compras

*(ideas 53, 54, 55, 56, 58, 69)*

## Viajes y viáticos

*(idea 63)*

## Rendición y auditoría

*(ideas 61, 62, 66, 67, 68)*
