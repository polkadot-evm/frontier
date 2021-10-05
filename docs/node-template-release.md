# Frontier Node Template Release Process

> NOTE: this based on the
> [Subtrate node template release process](https://github.com/paritytech/substrate/blob/master/docs/node-template-release.md) -

1.  Clone and checkout the `main` branch of the
    [Frontier Node Template](https://github.com/substrate-developer-hub/frontier-node-template/).
    Note the path to this directory.

2.  This release process has to be run in a github checkout Frontier directory with your work
    committed into `https://github.com/paritytech/frontier/`, because the build script will check
    the existence of your current git commit ID in the remote repository.

        Assume you are in the root directory of Frontier. Run:

        ```bash
        cd .maintain/
        ./node-template-release.sh TEMPLATE.tar.gz
        ```

3.  Expand the output tar gzipped file that is created in the top level working dir of Frontier and
    replace files in current Frontier Node Template by running the following command:

        ```bash
        # Note the file will be placed in the top level working dir of Frontier
        # Move the archive to wherever you like...
        tar xvzf TEMPLATE.tar.gz
        # This is where the tar.gz file uncompressed
        cd frontier-node-template
        # rsync with force copying. Note the slash at the destination directory is important
        rsync -avh * <destination node-template directory>/
        # For dry-running add `-n` argument
        # rsync -avhn * <destination node-template directory>/
        ```

        The above command only copies existing files from the source to the destination, but does not
        delete files/directories that are removed from the source. So you need to manually check and
        remove them in the destination.

4.  There are actually two packages in the Node Template, `frontier-node-template` (the node),
    `frontier-template-runtime` (the runtime); Each has its' own `Cargo.toml`. Inside these three
    files, dependencies are listed in expanded form and linked to a certain git commit in Frontier
    remote repository, such as:

        ```toml
        [dev-dependencies.sp-core]
        default-features = false
        git = 'https://github.com/paritytech/substrate.git'
        rev = 'c1fe59d060600a10eebb4ace277af1fee20bad17'
        version = '3.0.0'
        ```

        We will update each of them to the shortened form and link them to the Rust
        [crate registry](https://crates.io/). After confirming the versioned package is published in
        the crate, the above will become:

        ```toml
        [dev-dependencies]
        sp-core = { version = '3.0.0', default-features = false }
        ```

        P.S: This step can be automated if we update `node-template-release` package in
        `.maintain/node-template-release`.

5.  Once the three `Cargo.toml`s are updated, compile and confirm that the Node Template builds.
    Then commit the changes to a new branch in
    [Substrate Node Template](https://github.com/substrate-developer-hub/frontier-node-template),
    and make a PR.

        > Note that there is a chance the code in Substrate Node Template works with the linked Substrate git
        commit but not with published packages due to the latest (as yet) unpublished features. In this case,
        rollback that section of the Node Template to its previous version to ensure the Node Template builds.

6.  Once the PR is merged, tag the merged commit in master branch with the version number `vX.Y.Z+A`
    (e.g. `v3.0.0+1`). The `X`(major), `Y`(minor), and `Z`(patch) version number should follow
    Substrate release version. The last digit is any significant fixes made in the Substrate Node
    Template apart from Substrate. When the Substrate version is updated, this digit is reset to 0.

## Troubleshooting

-   Running the script `./node-template-release.sh <output tar.gz file>`, after all tests passed
    successfully, seeing the following error message:

        ```
        thread 'main' panicked at 'Creates output file: Os { code: 2, kind: NotFound, message: "No such file or directory" }', src/main.rs:250:10

    note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace ```

        This is likely due to that your output path is not a valid `tar.gz` filename or you don't have write
        permission to the destination. Try with a simple output path such as `~/node-tpl.tar.gz`.
