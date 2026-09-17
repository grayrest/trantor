# Sourced by the fixture scripts: HOST_TARGET is the roc target trantor builds
# for by default, which names the staging dir `platform/targets/<target>/`.
#
# Read from rustc's host triple rather than `uname`, because that is what
# trantor's own rule reads (src/host_target.rs): the OS and architecture it was
# built for, and on Linux the C environment. The scripts used to spell the
# staging dir `arm64mac`, which exists on no other host.
case "$(rustc -vV | sed -n 's/^host: //p')" in
	aarch64-apple-darwin) HOST_TARGET=arm64mac ;;
	x86_64-apple-darwin) HOST_TARGET=x64mac ;;
	aarch64-unknown-linux-gnu) HOST_TARGET=arm64glibc ;;
	x86_64-unknown-linux-gnu) HOST_TARGET=x64glibc ;;
	aarch64-unknown-linux-musl) HOST_TARGET=arm64musl ;;
	x86_64-unknown-linux-musl) HOST_TARGET=x64musl ;;
	*) echo "FAIL: no roc target known for rustc host '$(rustc -vV | sed -n 's/^host: //p')'"; exit 1 ;;
esac
