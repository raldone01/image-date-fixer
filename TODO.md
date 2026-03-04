# TODO

Add such an option:

--files0 path
         Read a list of paths to process from path, or '-' for stdin. The paths must be separated by NUL bytes. This is more robust if your file system allows newlines in file‐
         names (as POSIX does). Useful in conjunction with other tools that support it, e.g.:

         find . -type f -print0 | chafa --files0 -

         Can be specified multiple times. Any additional paths on the command line will be processed last.
