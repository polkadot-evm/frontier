# pallet-tkf-devteam

Esqueleto de pallet FRAME para Tokfinet.

## Funcionalidad
- Ejemplo básico de almacenamiento ()
- Ejemplo de extrinsic ()

## Uso
1. Añadir al workspace en el Cargo.toml raíz
2. Añadir al runtime con `scripts/add-pallet.sh pallet-tkf-devteam <índice>`

### 3️⃣ **DevTeam**

* Permite a los devs crear **DAppCells** (instancias de aplicaciones).
* Cada DAppCell puede transformarse en una **TokenFactory** (ERC20 / ERC721).
* La creación de un token **no es libre**, está mediada por este flujo → asegura calidad + evita spam.
* DevTeam recibe recompensas (devTokens, rewards DAO).