# Claves SSH del equipo

Cada archivo `<nombre>.pub` contiene **una** clave pública de un integrante que administra la VM de K3s. Terraform (`infra/terraform/acceso.tf`) escribe `authorized_keys` del usuario `azureuser` con la clave del administrador más estas. Quitar un archivo revoca el acceso en el siguiente apply.

Las claves públicas pueden estar en un repositorio público. La clave privada (el archivo sin `.pub`) **nunca** se comparte ni se versiona.

## Dar acceso a un integrante

1. El integrante genera su par de claves, si no lo tiene (`ssh-keygen -t ed25519`), y obtiene su IPv4 pública (`curl -4 https://api.ipify.org`).
2. PR con `infra/ssh/<nombre>.pub`. El plan muestra la extensión `team-ssh-keys` actualizada en sitio.
3. Un administrador agrega la IP `/32` al secret `INFRA_ADMIN_CIDRS` (y a su `terraform.tfvars` local). La IP no se publica en el repositorio.
4. Con la VM **encendida** (Azure solo cambia extensiones con la VM en marcha), se aprueba el apply.
5. El integrante compara la huella del servidor antes de aceptarla (`bash infra/scripts/vm.sh fingerprint` o la que le pase el administrador) y entra con `ssh azureuser@<fqdn>`. Dentro de la VM usa `sudo k3s kubectl`.
