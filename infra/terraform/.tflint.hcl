# tflint (DEV-08): reglas del lenguaje de Terraform con el preset recomendado
# (tipos de variables, declaraciones sin uso, versiones requeridas, sintaxis obsoleta).
plugin "terraform" {
  enabled = true
  preset  = "recommended"
}
