import sys
import os

"""
Strips those pesky tera types
"""
if __name__ == '__main__':
	if len(sys.argv) != 2:
		print("Usage: draft_parser.py <path to draft_sets>")
	draft_file = sys.argv[1]

	current_data = []
	i = 0
	with open(draft_file, "r+") as f:
		lines = f.readlines()
		f.seek(0)
		for line in lines:
			if "Tera Type" not in line:
				f.write(line)
		f.truncate()
				
