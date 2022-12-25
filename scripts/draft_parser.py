import sys
import os

if __name__ == '__main__':
	if len(sys.argv) != 3:
		print("Usage: draft_parser.py <path to draft_sets> <output_dir>")
	draft_file = sys.argv[1]
	output_dir = sys.argv[2]

	current_data = []
	i = 0
	with open(draft_file) as f:
		for line in f:
			if len(line.strip()) == 0:
				with open(f"{output_dir}/{i}.txt", 'w') as w:
					w.writelines(current_data)
				current_data.clear()
				i += 1
			else:
				current_data.append(line)
		with open(f"{output_dir}/{i}.txt", 'w') as w:
			w.writelines(current_data)
				


