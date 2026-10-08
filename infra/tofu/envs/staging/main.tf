# Staging edge: Cloudflare DNS for the Fly apps. The API token comes from
# CLOUDFLARE_API_TOKEN in the environment (DNS Edit on tryzoen.com), never from a file.
terraform {
  required_version = ">= 1.8"
  required_providers {
    cloudflare = {
      source  = "cloudflare/cloudflare"
      version = "~> 5.0"
    }
  }
}

provider "cloudflare" {}

variable "zone_id" {
  description = "tryzoen.com"
  type        = string
  default     = "51e1c8fd89883bf4ad028b573d577b3c"
}

module "edge_dns" {
  source  = "../../modules/edge-dns"
  zone_id = var.zone_id
  origin  = "zoen-staging-relay.fly.dev"
  hosts   = ["relay", "api", "media", "id"]
}

output "hosts" {
  value = module.edge_dns.fqdns
}
