
5.1() Implementar un sistema de reputación básico (TKFr):
		Crear un pallet de reputación: En lugar de una lógica de consenso compleja, crea un pallet simple llamado, por ejemplo, pallet-reputation.
		Definir la lógica de ganar reputación: Este pallet podría tener una función básica que permita a un usuario ganar una pequeña cantidad de TKFr (por ejemplo, mint_reputation(account, amount)). 
			Esta función podría ser llamada por un administrador por ahora.
		Definir la lógica de perder reputación: El pallet también debería tener una función para reducir el saldo de TKFr de una cuenta si no ha participado activamente.
		Objetivo del MVP: Demostrar que el token TKFr tiene un mecanismo de emisión y un propósito, lo que establece la base para la lógica de consenso futura.