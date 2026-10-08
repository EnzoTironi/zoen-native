terraform {
  required_providers {
    cloudflare = {
      source  = "cloudflare/cloudflare"
      version = "~> 5.0"
    }
  }
}

variable "zone_id" {
  type = string
}

variable "origin" {
  description = "Hostname every record points at, e.g. zoen-staging-relay.fly.dev."
  type        = string
}

variable "hosts" {
  description = "Subdomain labels to create. Never the zone root."
  type        = set(string)
  validation {
    condition     = alltrue([for h in var.hosts : h != "" && h != "@" && !strcontains(h, ".")])
    error_message = "Hosts are single subdomain labels; the zone root is never managed here."
  }
}

variable "proxied" {
  description = "Behind Cloudflare's proxy. Needs the zone's SSL mode at Full (strict) so the origin keeps its own certificate."
  type        = bool
  default     = false
}

data "cloudflare_zone" "this" {
  zone_id = var.zone_id
}

resource "cloudflare_dns_record" "host" {
  for_each = var.hosts
  zone_id  = var.zone_id
  name     = "${each.key}.${data.cloudflare_zone.this.name}"
  type     = "CNAME"
  content  = var.origin
  proxied  = var.proxied
  ttl      = var.proxied ? 1 : 300
  comment  = "zoen staging, managed by OpenTofu (infra/tofu)"
}

output "fqdns" {
  value = [for r in cloudflare_dns_record.host : r.name]
}
