# gvi

A Small CLI wrapper of gvim with several useful features.


## What is the difference between gvim and gvi?

This program makes it easy to:

- open files while reusing an existing gvim instance (if it does).
    - if not, a new gvim instance will be created when performed.

- try to recursively open all the files in a specified directory.
    - it will cause a failure if:
        - the number of the targets is too great.
        - expansion size is too large.


## Prerequisite

gvim


## Usage 

To open files:

`gvi <file_1> <file_2> <file_3> ...` 

To recursively open all the files in a directory:

`gvi <directory>` 


