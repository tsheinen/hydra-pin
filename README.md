# hydra-pin

`hydra-pin` is a tool to query Nix's Hydra CI for the last successful build of a package and generate an overlay pinning only that package to that version. 

## usage

```
Usage: hydra-pin [OPTIONS] --package <PACKAGE> --nix <NIX> <COMMAND>

Commands:
  pin
  unpin
  help   Print this message or the help of the given subcommand(s)

Options:
  -b, --hydra-check <HYDRA_CHECK>  hydra-check binary to use [env: HYDRA_CHECK=]
  -p, --package <PACKAGE>          Packag
  -n, --nix <NIX>                  Nix file to store generated overlay in
  -h, --help                       Print help
```

`hydra-pin -n /etc/nixos/generated/pinned.nix -p sage pin` will generate a Nix file like so:

```nix
# sage https://github.com/NixOS/nixpkgs/archive/63c3a29ca82437c87573e4c6919b09a24ea61b0f.tar.gz 0inlj292qm3k4sqibm60gpdh3kq57vvl3mjh2xpr9svjpfcz5hz1

{pkgs}: {
    overlay = (final: prev: {
sage = (import (fetchTarball {
            url = "https://github.com/NixOS/nixpkgs/archive/63c3a29ca82437c87573e4c6919b09a24ea61b0f.tar.gz";
            sha256 = "0inlj292qm3k4sqibm60gpdh3kq57vvl3mjh2xpr9svjpfcz5hz1";
        }) { system = pkgs.system; }).sage;
        
        
    });
}
```

which can be imported as an overlay, transparently replacing the broken version in nixpkgs-unstable. 

```nix
{ pkgs}:

{
  config = {
    nixpkgs.overlays =
      [
        ((import ../generated/pinned.nix) { inherit pkgs; }).overlay
      ];
  };
}
```