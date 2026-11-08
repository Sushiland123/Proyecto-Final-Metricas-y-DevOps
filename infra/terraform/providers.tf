provider "azurerm" {
  features {}
  subscription_id = var.subscription_id

  # Las identidades de CI no pueden registrar resource providers (la de plan es
  # Reader): el bootstrap registra los que usa esta configuración.
  resource_provider_registrations = "none"
}
