import sys
import os
from bs4 import BeautifulSoup


def check_image_exists(asset_folder, image):
    filename = image.split("/")[-1]
    return os.path.isfile(f"{asset_folder}/{filename}")

def fix_image_path(current_path, target_root):
    filename = current_path.split("/")[-1]
    return f"{target_root}/{filename}"

def make_template_entrypoint():
    template_entrypoint = soup.new_tag("p")
    template_entrypoint.string = "{{{template_entrypoint}}}"
    return template_entrypoint

if __name__ == '__main__':
    if len(sys.argv) != 4:
        print("Usage: pokepaste_parser.py <path to html file> <path to assets dir> <output_dir>")
    with open(sys.argv[1]) as html_file:
        soup = BeautifulSoup(html_file, 'html.parser')

    asset_dir = sys.argv[2]
    output_dir = sys.argv[3]

    draft_items = []
    for element in soup.body:
        # Pokemon descriptions are contained within 'article'
        if element.name == 'article':
            pokemon_name = element.pre.span.contents[0]
            # Fix image paths
            for x in element.div:
                if x.name == 'img':
                    image_type = x.attrs['class']
                    if 'img-pokemon' in image_type or 'img-item' in image_type:
                        image_path = x.attrs["src"]
                        if not check_image_exists(asset_dir, image_path):
                            print(f"Missing asset {image_path} for {pokemon_name} {image_type}")
                        fixed_path = fix_image_path(image_path, "static/assets")
                        x.attrs["src"] = fixed_path
            draft_items.append(element)


    for draft_id, element in enumerate(draft_items):
        with open(f"{output_dir}/{draft_id}.html", 'w') as file:
            for contents in element.contents:
                file.write(str(contents))

            

    

    
    