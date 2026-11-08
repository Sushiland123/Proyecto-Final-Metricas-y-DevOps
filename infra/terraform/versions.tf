terraform {
  required_version = ">= 1.12.0"

  # Configuración en backend.hcl: terraform init -backend-config=backend.hcl
  backend "azurerm" {}

  required_providers {
    azurerm = {
      source = "hashicorp/azurerm"
      # Versión validada por el docente en mexicocentral (k8s-azure-template).
      # Con versiones que usan la API de red 2025-01-01, Azure devolvió 404 al
      # leer recursos recién creados; aquí ocurrió con la VNet (docs/infra/README.md).
      version = "= 4.40.0"
    }
  }
}
