# pallet-tkf-orchestra

Esqueleto de pallet FRAME para Tokfinet.

## Funcionalidad
- Ejemplo básico de almacenamiento ()
- Ejemplo de extrinsic ()

## Uso
1. Añadir al workspace en el Cargo.toml raíz
2. Añadir al runtime con `scripts/add-pallet.sh pallet-tkf-orchestra <índice>`

### 6️⃣ **Orchestra**

* Es el **director de orquesta del runtime**:

  * Coordina entre Master, cAuth, cTeam y Foundation.
  * Balancea nodos y requerimientos.
  * Monitorea el sistema (salud de nodos, carga, latencia).
  * Decide qué procesos deben ejecutarse en paralelo o secuencial.
* Piensa en él como un **scheduler + load balancer** especializado para Tokfin.
