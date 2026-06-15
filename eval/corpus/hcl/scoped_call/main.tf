module "vpc" {
  source = "./vpc"
}

resource "aws_instance" "web" {
  vpc_id = module.vpc.id
}
