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
    if len(sys.argv) != 3:
        print("Usage: pokepaste_parser.py <path to html file> <path to assets dir> <output_dir>")
    with open(sys.argv[1]) as html_file:
        soup = BeautifulSoup(html_file, 'html.parser')

    output_dir = sys.argv[2]

    draft_items = []
    for element in soup.div.ol:
        # Pokemon descriptions are contained within 'article'
        if "value" in element.attrs:
            draft_id = int(element.attrs["value"])
            print(draft_id)
            set_chart = element.find_all("div", {"class": "setchart"})[0]
            stats_chart = element.find_all("div", {"class": "setcol setcol-stats"})[0]
            statrows = []
            for stat in stats_chart.div.button:
                if "statrow-head" in stat.attrs["class"]:
                    continue
                statrows.append(stat)

            for stat in statrows:
                stat.em.extract()
                if stat.small:
                    stat.small.extract()
            with open(f"{output_dir}/{draft_id}.html", 'w') as file:
                for contents in statrows:
                    file.write(str(contents))
    

    
    