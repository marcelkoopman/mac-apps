import os


def list_files(folder):
	for name in os.listdir(folder):
        print(name)
