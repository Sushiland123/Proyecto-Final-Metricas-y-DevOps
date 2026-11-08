terraform {
  required_version = ">= 1.12.0"

  # Configuración en backend.hcl: terraform init -backend-config=backend.hcl
  backend "azurerm" {}

  required_providers {
    azurerm = {
      source  = "hashicorp/azurerm"
      version = "~> 5.10"
    }
  }
}
