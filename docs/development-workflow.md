# Development workflow

## Pull request

All changes (except new releases) are handled through pull requests.

## Versioning

Frontier follows [Semantic Versioning](https://semver.org/).
An unreleased crate in the repository will have the `-dev` suffix in the end, and we do rolling releases.

When you make a pull request against this repository, please also update the affected crates' versions, using the following rules.
Note that the rules should be applied recursively -- if a change modifies any upper crate's dependency (even just the `Cargo.toml` file),
then the upper crate will also need to apply those rules.

Additionally, if your change is notable, then you should also modify the corresponding `CHANGELOG.md` file, in the "Unreleased" section.

If the affected crate already has `-dev` suffix:

* If your change is a patch, then you do not have to update any versions.
* If your change introduces a new feature, please check if the local version already had its minor version bumped, if not, bump it.
* If your change modifies the current interface, please check if the local version already had its major version bumped, if not, bump it.

If the affected crate does not yet have `-dev` suffix:

* If your change is a patch, then bump the patch version, and add `-dev` suffix.
* If your change introduces a new feature, then bump the minor version, and add `-dev` suffix.
* If your change modifies the current interface, then bump the major version, and add `-dev` suffix.

If your pull request introduces a new crate, please set its version to `1.0.0-dev`.
