//! What kind of file an extension says it is, to colour the treemap and group the extension list.

/// A broad kind of file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    Image,
    Video,
    Audio,
    Archive,
    Document,
    Code,
    Program,
    Data,
    System,
    Other,
}

impl Category {
    pub const ALL: [Category; 10] = [
        Category::Image,
        Category::Video,
        Category::Audio,
        Category::Archive,
        Category::Document,
        Category::Code,
        Category::Program,
        Category::Data,
        Category::System,
        Category::Other,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Category::Image => "Pictures",
            Category::Video => "Video",
            Category::Audio => "Audio",
            Category::Archive => "Archives and disk images",
            Category::Document => "Documents",
            Category::Code => "Source code",
            Category::Program => "Programs and libraries",
            Category::Data => "Databases and data",
            Category::System => "System, logs and temporary",
            Category::Other => "Other",
        }
    }

    /// The category of a file named `name`.
    pub fn of_name(name: &str) -> Category {
        crate::stats::extension_of(name).map_or(Category::Other, |e| Category::of_extension(&e))
    }

    /// The category of a lower-case extension without its dot.
    pub fn of_extension(ext: &str) -> Category {
        match ext {
            "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tif" | "tiff" | "webp" | "heic" | "heif" | "avif" | "raw" | "cr2" | "cr3" | "nef" | "arw"
            | "dng" | "orf" | "rw2" | "psd" | "xcf" | "svg" | "ico" | "icns" | "jxl" | "exr" | "hdr" | "kra" => Category::Image,
            "mp4" | "m4v" | "mov" | "mkv" | "avi" | "wmv" | "webm" | "flv" | "mpg" | "mpeg" | "m2ts" | "mts" | "ts" | "vob" | "3gp" | "braw"
            | "r3d" => Category::Video,
            "mp3" | "m4a" | "aac" | "flac" | "wav" | "ogg" | "opus" | "wma" | "aif" | "aiff" | "alac" | "mid" | "midi" => Category::Audio,
            "zip" | "7z" | "rar" | "gz" | "tgz" | "bz2" | "xz" | "zst" | "tar" | "cab" | "iso" | "img" | "dmg" | "vhd" | "vhdx" | "vmdk"
            | "qcow2" | "vdi" | "wim" | "esd" | "lz" | "lz4" | "lzma" | "pkg" | "deb" | "rpm" | "apk" | "appimage" | "snap" | "flatpak" | "jar"
            | "war" => Category::Archive,
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp" | "rtf" | "txt" | "md" | "epub" | "pages"
            | "numbers" | "key" | "csv" | "tex" | "one" | "pst" | "ost" | "msg" | "eml" | "mbox" => Category::Document,
            "rs" | "c" | "h" | "cpp" | "hpp" | "cc" | "cs" | "java" | "kt" | "go" | "py" | "js" | "mjs" | "tsx" | "jsx" | "rb" | "php" | "swift"
            | "m" | "mm" | "sh" | "ps1" | "bat" | "cmd" | "lua" | "pl" | "scala" | "html" | "htm" | "css" | "scss" | "vue" | "toml" | "yaml"
            | "yml" | "xml" | "json" | "ipynb" | "sql" | "gradle" | "cmake" | "make" | "pdb" | "rlib" | "rmeta" | "o" | "obj" | "class" | "pyc"
            | "d" => Category::Code,
            "exe" | "dll" | "msi" | "msix" | "appx" | "so" | "dylib" | "a" | "lib" | "app" | "bin" | "elf" | "efi" | "node" | "wasm" | "ocx"
            | "drv" | "mui" => Category::Program,
            "db" | "sqlite" | "sqlite3" | "mdb" | "accdb" | "ldf" | "mdf" | "ndf" | "bak" | "dat" | "parquet" | "arrow" | "h5" | "hdf5" | "npy"
            | "npz" | "pt" | "pth" | "onnx" | "safetensors" | "gguf" | "ckpt" | "mat" | "sav" => Category::Data,
            "sys" | "log" | "tmp" | "temp" | "cache" | "etl" | "evtx" | "dmp" | "mdmp" | "swp" | "old" | "lock" | "pf" | "cat" | "manifest"
            | "regtrans-ms" | "blf" | "ttf" | "otf" | "woff" | "woff2" | "fon" => Category::System,
            _ => Category::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories() {
        assert_eq!(Category::of_name("Holiday.JPG"), Category::Image);
        assert_eq!(Category::of_name("movie.mkv"), Category::Video);
        assert_eq!(Category::of_name("backup.tar.gz"), Category::Archive);
        assert_eq!(Category::of_name("pagefile.sys"), Category::System);
        assert_eq!(Category::of_name("main.rs"), Category::Code);
        assert_eq!(Category::of_name("model.gguf"), Category::Data);
        assert_eq!(Category::of_name("README"), Category::Other);
        assert_eq!(Category::of_name(".bashrc"), Category::Other);
        assert_eq!(Category::ALL.len(), 10);
    }
}
