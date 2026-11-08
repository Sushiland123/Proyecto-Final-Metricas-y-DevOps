# Claves SSH del equipo

Cada archivo `<nombre>.pub` contiene **una** clave pública de un integrante que administra la VM de K3s. Terraform (`infra/terraform/acceso.tf`) escribe `authorized_keys` del usuario `azureuser` con la clave del administrador más estas. Quitar un archivo revoca el acceso en el siguiente apply.

Las claves públicas pueden estar en un repositorio público. La clave privada (el archivo sin `.pub`) **nunca** se comparte ni se versiona.

## Dar acceso a un integrante

1. El integrante genera su par de claves, si no lo tiene (`ssh-keygen -t ed25519`), y obtiene su IPv4 pública (`curl -4 https://api.ipify.org`).
2. PR con `infra/ssh/<nombre>.pub`. El plan muestra la extensión `team-ssh-keys` actualizada en sitio. Al integrarlo, el push a `main` lanza el workflow `Terraform`, que también vigila `infra/ssh/**`. Si no corriera, se lanza a mano desde `main`: **Actions → Terraform → Run workflow**.
3. Un administrador agrega la IP `/32` al secret `INFRA_ADMIN_CIDRS` (y a su `terraform.tfvars` local). La IP no se publica en el repositorio.
4. Con la VM **encendida** (Azure solo cambia extensiones con la VM en marcha), se aprueba el apply.
5. El integrante compara la huella del servidor antes de aceptarla (`bash infra/scripts/vm.sh fingerprint` o la que le pase el administrador) y entra con `ssh azureuser@<fqdn>`. Dentro de la VM usa `sudo k3s kubectl`.

## Quitar el acceso

1. PR que elimina `infra/ssh/<nombre>.pub` y, en el mismo momento, retirar su IP del secret `INFRA_ADMIN_CIDRS`. Con la VM encendida, se aprueba el apply: `authorized_keys` queda sin esa clave.
2. **El apply no corta las conexiones ya abiertas.** Las sesiones y los túneles SSH establecidos siguen vivos hasta que se cierran. En la VM, listar las conexiones al puerto 22 con su IP de origen y su proceso, y terminar las del integrante:

   ```bash
   sudo ss -tnp state established '( sport = :22 )'
   sudo kill <PID de sshd de esa conexión>
   ```

3. **Quitar la clave no invalida un kubeconfig que el integrante haya copiado.** El de `~/.kube/config` usa el certificado de administrador del clúster. Si lo copió, rotar ese certificado y volver a copiar el kubeconfig de quienes conservan acceso. El procedimiento es `k3s certificate rotate`, con K3s detenido:

   ```bash
   sudo systemctl stop k3s
   sudo k3s certificate rotate --service admin
   sudo systemctl start k3s
   sudo install -m 0600 -o azureuser -g azureuser /etc/rancher/k3s/k3s.yaml ~azureuser/.kube/config
   ```
