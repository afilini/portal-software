{ pkgs, packages, devShells, getRust }:

let
  qemuOnlyArm = packages.smallQemu;
in
pkgs.dockerTools.buildNixShellImage {
  name = "portal-dev-environment";
  tag = "latest";
  homeDirectory = "/app";
  drv = devShells.embedded;
}
