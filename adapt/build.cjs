#!/usr/bin/env node
/**
 * Adapt.js 构建脚本
 * 将 src/ 模块打包压缩为 adapt.min.js
 */

const esbuild = require('esbuild');
const fs = require('fs');
const path = require('path');

async function build() {
  console.log('Building adapt.min.js from modules...');

  const srcDir = path.join(__dirname, 'src');
  const entryFile = path.join(srcDir, 'index.js');
  const outputFile = path.join(__dirname, '..', 'adapt.min.js');

  if (!fs.existsSync(entryFile)) {
    console.error('Error: adapt/src/index.js not found');
    process.exit(1);
  }

  try {
    // 计算源文件总大小
    let totalSize = 0;
    const srcFiles = fs.readdirSync(srcDir).filter(f => f.endsWith('.js'));
    for (const file of srcFiles) {
      totalSize += fs.statSync(path.join(srcDir, file)).size;
    }
    console.log(`Source modules: ${srcFiles.length} files, ${(totalSize / 1024).toFixed(2)} KB`);

    await esbuild.build({
      entryPoints: [entryFile],
      bundle: true,      // 打包所有模块
      minify: true,      // 压缩
      format: 'iife',    // 立即执行函数
      target: ['es2020'],
      outfile: outputFile,
      banner: {
        js: `/*! PWA Adapt Bridge | Built: ${new Date().toISOString()} */`,
      },
    });

    // 显示压缩后大小
    const minifiedStats = fs.statSync(outputFile);
    console.log(`Minified: ${(minifiedStats.size / 1024).toFixed(2)} KB`);
    console.log(`Saved: ${((1 - minifiedStats.size / totalSize) * 100).toFixed(1)}%`);
    console.log(`Output: adapt.min.js`);
    
  } catch (error) {
    console.error('Build failed:', error);
    process.exit(1);
  }
}

build();
