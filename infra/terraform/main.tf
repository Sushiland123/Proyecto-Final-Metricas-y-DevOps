locals {
  prefix = "eventos-${var.name_suffix}"
  tags = {
    project    = "eventos"
    scenario   = "3-ingesta-analisis-eventos"
    managed_by = "terraform"
  }
}

resource "azurerm_resource_group" "main" {
  name     = "rg-${local.prefix}"
  location = var.location
  tags     = local.tags
}

# ---------------------------------------------------------------- red

resource "azurerm_virtual_network" "main" {
  name                = "vnet-${local.prefix}"
  location            = azurerm_resource_group.main.location
  resource_group_name = azurerm_resource_group.main.name
  address_space       = ["10.40.0.0/16"]
  tags                = local.tags
}

resource "azurerm_subnet" "k3s" {
  name                 = "snet-k3s"
  resource_group_name  = azurerm_resource_group.main.name
  virtual_network_name = azurerm_virtual_network.main.name
  address_prefixes     = ["10.40.1.0/24"]
}

# Exposición mínima: HTTP/HTTPS públicos para el Ingress de K3s, SSH solo
# desde los rangos de administración y la API de Kubernetes (6443) cerrada.
resource "azurerm_network_security_group" "k3s" {
  name                = "nsg-${local.prefix}-k3s"
  location            = azurerm_resource_group.main.location
  resource_group_name = azurerm_resource_group.main.name
  tags                = local.tags

  security_rule {
    name                       = "AllowHttpInternet"
    priority                   = 100
    direction                  = "Inbound"
    access                     = "Allow"
    protocol                   = "Tcp"
    source_port_range          = "*"
    destination_port_range     = "80"
    source_address_prefix      = "Internet"
    destination_address_prefix = "*"
  }

  security_rule {
    name                       = "AllowHttpsInternet"
    priority                   = 110
    direction                  = "Inbound"
    access                     = "Allow"
    protocol                   = "Tcp"
    source_port_range          = "*"
    destination_port_range     = "443"
    source_address_prefix      = "Internet"
    destination_address_prefix = "*"
  }

  security_rule {
    name                       = "AllowSshAdmin"
    priority                   = 120
    direction                  = "Inbound"
    access                     = "Allow"
    protocol                   = "Tcp"
    source_port_range          = "*"
    destination_port_range     = "22"
    source_address_prefixes    = var.admin_cidrs
    destination_address_prefix = "*"
  }

  # Explícita para documentar la intención; la regla por defecto también la bloquea.
  security_rule {
    name                       = "DenyKubernetesApi"
    priority                   = 400
    direction                  = "Inbound"
    access                     = "Deny"
    protocol                   = "Tcp"
    source_port_range          = "*"
    destination_port_range     = "6443"
    source_address_prefix      = "*"
    destination_address_prefix = "*"
  }
}

resource "azurerm_subnet_network_security_group_association" "k3s" {
  subnet_id                 = azurerm_subnet.k3s.id
  network_security_group_id = azurerm_network_security_group.k3s.id
}

# La etiqueta DNS da un nombre estable <label>.<región>.cloudapp.azure.com para
# el Ingress y su certificado TLS.
resource "azurerm_public_ip" "k3s" {
  name                = "pip-${local.prefix}-k3s"
  location            = azurerm_resource_group.main.location
  resource_group_name = azurerm_resource_group.main.name
  allocation_method   = "Static"
  sku                 = "Standard"
  domain_name_label   = local.prefix
  tags                = local.tags
}

resource "azurerm_network_interface" "k3s" {
  name                = "nic-${local.prefix}-k3s"
  location            = azurerm_resource_group.main.location
  resource_group_name = azurerm_resource_group.main.name
  tags                = local.tags

  ip_configuration {
    name                          = "primary"
    subnet_id                     = azurerm_subnet.k3s.id
    private_ip_address_allocation = "Dynamic"
    public_ip_address_id          = azurerm_public_ip.k3s.id
  }
}

# ---------------------------------------------------------------- VM con K3s

# cloud-init instala K3s en el primer arranque. Cambiar el template o la
# versión de K3s reemplaza la VM (y los volúmenes local-path): el plan lo muestra.
resource "azurerm_linux_virtual_machine" "k3s" {
  name                            = "vm-${local.prefix}-k3s"
  computer_name                   = "k3s-eventos"
  location                        = azurerm_resource_group.main.location
  resource_group_name             = azurerm_resource_group.main.name
  size                            = var.vm_size
  admin_username                  = var.admin_username
  disable_password_authentication = true
  network_interface_ids           = [azurerm_network_interface.k3s.id]
  tags                            = local.tags

  admin_ssh_key {
    username   = var.admin_username
    public_key = var.admin_ssh_public_key
  }

  os_disk {
    name                 = "osdisk-${local.prefix}-k3s"
    caching              = "ReadWrite"
    storage_account_type = "StandardSSD_LRS"
    disk_size_gb         = var.os_disk_size_gb
  }

  source_image_reference {
    publisher = "Canonical"
    offer     = "ubuntu-24_04-lts"
    sku       = "server"
    version   = "latest"
  }

  custom_data = base64encode(templatefile("${path.module}/cloud-init.yaml.tftpl", {
    k3s_version    = var.k3s_version
    admin_username = var.admin_username
  }))
}

# El apagado desasigna la VM: deja de cobrarse el cómputo; el disco y la IP
# pública estática siguen generando cargo.
resource "azurerm_dev_test_global_vm_shutdown_schedule" "k3s" {
  virtual_machine_id    = azurerm_linux_virtual_machine.k3s.id
  location              = azurerm_resource_group.main.location
  enabled               = true
  daily_recurrence_time = var.auto_shutdown_time
  timezone              = var.auto_shutdown_timezone
  tags                  = local.tags

  notification_settings {
    enabled = false
  }
}

# ---------------------------------------------------------------- costos

data "azurerm_subscription" "current" {}

# El límite del proyecto es el consumo agregado de la suscripción. Las alertas
# notifican, pero no detienen recursos.
resource "azurerm_consumption_budget_subscription" "project" {
  count           = var.create_budget ? 1 : 0
  name            = "budget-${local.prefix}"
  subscription_id = data.azurerm_subscription.current.id
  amount          = var.budget_amount
  time_grain      = "Monthly"

  time_period {
    start_date = var.budget_start_date
  }

  notification {
    enabled        = true
    threshold      = 50
    operator       = "GreaterThanOrEqualTo"
    threshold_type = "Actual"
    contact_emails = var.budget_contact_emails
  }

  notification {
    enabled        = true
    threshold      = 80
    operator       = "GreaterThanOrEqualTo"
    threshold_type = "Actual"
    contact_emails = var.budget_contact_emails
  }

  notification {
    enabled        = true
    threshold      = 100
    operator       = "GreaterThanOrEqualTo"
    threshold_type = "Forecasted"
    contact_emails = var.budget_contact_emails
  }

  lifecycle {
    precondition {
      condition     = length(var.budget_contact_emails) > 0
      error_message = "budget_contact_emails necesita al menos un correo cuando create_budget = true."
    }
  }
}
