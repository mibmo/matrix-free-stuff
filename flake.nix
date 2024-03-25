{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.rust-analyzer-src.follows = "";
    };

    flake-utils.url = "github:numtide/flake-utils";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        flake-utils.follows = "flake-utils";
      };
    };

    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, crane, fenix, flake-utils, rust-overlay, advisory-db, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        inherit (pkgs) lib;

        craneLib = (crane.mkLib pkgs).overrideToolchain pkgs.rust-bin.nightly.latest.default;
        src = craneLib.cleanCargoSource (craneLib.path ./.);

        commonArgs = {
          inherit src;
          strictDeps = true;

          buildInputs = with pkgs; [
            pkg-config
            openssl
          ] ++ lib.optionals pkgs.stdenv.isDarwin [
            libiconv
          ];
        };

        craneLibLLvmTools = craneLib.overrideToolchain
          (fenix.packages.${system}.complete.withComponents [
            "cargo"
            "llvm-tools"
            "rustc"
          ]);

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        matrix-free-stuff = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
        });
      in
      {
        checks = {
          # Build the crate as part of `nix flake check` for convenience
          inherit matrix-free-stuff;

          # Run clippy (and deny all warnings) on the crate source,
          # again, reusing the dependency artifacts from above.
          #
          # Note that this is done as a separate derivation so that
          # we can block the CI if there are issues here, but not
          # prevent downstream consumers from building our crate by itself.
          matrix-free-stuff-clippy = craneLib.cargoClippy (commonArgs // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--all-targets -- --deny warnings";
          });

          matrix-free-stuff-doc = craneLib.cargoDoc (commonArgs // {
            inherit cargoArtifacts;
          });

          # Check formatting
          matrix-free-stuff-fmt = craneLib.cargoFmt {
            inherit src;
          };

          # Audit dependencies
          matrix-free-stuff-audit = craneLib.cargoAudit {
            inherit src advisory-db;
          };

          # Audit licenses
          matrix-free-stuff-deny = craneLib.cargoDeny {
            inherit src;
          };

          # Run tests with cargo-nextest
          # Consider setting `doCheck = false` on `matrix-free-stuff` if you do not want
          # the tests to run twice
          matrix-free-stuff-nextest = craneLib.cargoNextest (commonArgs // {
            inherit cargoArtifacts;
            partitions = 1;
            partitionType = "count";
          });
        };

        packages = {
          default = matrix-free-stuff;
          inherit matrix-free-stuff;
        } // lib.optionalAttrs (!pkgs.stdenv.isDarwin) {
          matrix-free-stuff-llvm-coverage = craneLibLLvmTools.cargoLlvmCov (commonArgs // {
            inherit cargoArtifacts;
          });
        };

        apps.default = flake-utils.lib.mkApp {
          drv = matrix-free-stuff;
        };

        devShells.default =
          let
            host = "localhost";
            port = 8008;

            registrationPath = "./registration.yaml";

            conduitPath = path: "/tmp/matrix-free-stuff-conduit/" + path;
            conduit-config = (pkgs.formats.toml { }).generate "matrix-free-stuff-conduit.toml" {
              global = {
                server_name = "localhost";
                address = "127.0.0.1";
                inherit port;
                trusted_servers = [ "matrix.org" ];

                database_backend = "rocksdb";
                database_path = conduitPath "database";

                allow_registration = true;
                allow_federation = false;
                allow_check_for_updates = false;

                enable_lightning_bolt = false;
              };
            };

            synapsePath = path: "/tmp/matrix-free-stuff-synapse/" + path;
            synapse-config = (pkgs.formats.yaml { }).generate "matrix-free-stuff-synapse.yaml" {
              server_name = "localhost";
              pid_file = synapsePath "homeserver.pid";
              listeners = [{
                bind_addresses = [ "127.0.0.1" "::1" ];
                inherit port;
                tls = false;
                type = "http";
                x_forwarded = true;
                resources = [ "client" ];
              }];

              database = {
                name = "sqlite3";
                args.database = synapsePath "homeserver.db";
              };

              log_config = synapsePath "log.config";
              media_store_path = synapsePath "media_store";

              enable_registration = true;
              enable_registration_without_verification = true;
              report_stats = false;

              macaroon_secret_key = "dYdxRkpyg6R5x=V+3pWeVcmw@oUBUaOI=GHFa.M,Drtzd9UN6F";
              form_secret = "jYXbtfA5eknw0WdFdt384rp_Bdq-0DHtzhotM8=uArj:7;KCAR";
              signing_key_path = synapsePath "signing.key";
              trusted_key_servers = [{ server_name = "matrix.org"; }];

              app_service_config_files = [ registrationPath ];
            };
          in
          craneLib.devShell {
            checks = self.checks.${system};
            packages = with pkgs; [ matrix-conduit matrix-synapse ];

            RUST_LOG = "matrix_free_stuff=trace";
            HOMESERVER_URL = "http://${host}:${toString port}";
            APPSERVICE_REGISTRATION = registrationPath;

            CONDUIT_CONFIG = conduit-config;
            SYNAPSE_CONFIG = synapse-config;
          };
      });
}
