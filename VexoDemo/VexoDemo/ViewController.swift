//
//  ViewController.swift
//  VexoDemo
//
//  Created by peiyan_wang on 2025/11/30.
//

import UIKit
import shared_app
import MetalKit

class VexoView: MTKView {
    private var uiEngine: MobileApp?
    
    required override init(frame frameRect: CGRect, device: (any MTLDevice)?) {
        super.init(frame: frameRect, device: device)
        self.delegate = self
        self.device = device
        self.colorPixelFormat = .bgra8Unorm_srgb
        self.clearColor = MTLClearColor(red: 0.0, green: 0.0, blue: 0.0, alpha: 1.0)
        setupUiEngine()
    }
    
    required init(coder: NSCoder) {
        fatalError("Not implemented")
    }
    
    private func setupUiEngine() {
        let layerPtr = unsafeBitCast(self.layer, to: UInt64.self)
        let engine = MobileApp()
        self.uiEngine = engine
        let scale = self.traitCollection.displayScale
        print("scale: \(scale)")
        
        Task {
            engine.startUiThread(
                viewPtrAsU64: layerPtr,
                width: UInt32(self.bounds.size.width),
                height: UInt32(self.bounds.size.height),
                scaleFactor: Float(scale)
            )
        }
        self.mtkView(self, drawableSizeWillChange: self.drawableSize)
    }
}

extension VexoView: MTKViewDelegate {
    func mtkView(_ view: MTKView, drawableSizeWillChange size: CGSize) {
        guard let uiEngine else {
            return
        }
        uiEngine.resize(width: UInt32(size.width), height: UInt32(size.height))
        uiEngine.render()
        view.setNeedsDisplay()
    }
    
    func draw(in view: MTKView) {
        guard let uiEngine else {
            return
        }
        
        uiEngine.render()
    }
}

class ViewController: UIViewController {
    
    private var vexoView: VexoView!

    override func viewDidLoad() {
        super.viewDidLoad()
        // Do any additional setup after loading the view.
        vexoView = VexoView(frame: self.view.bounds, device: MTLCreateSystemDefaultDevice())
        vexoView.isPaused = false
        vexoView.preferredFramesPerSecond = 60
        vexoView.translatesAutoresizingMaskIntoConstraints = false
        self.view .addSubview(vexoView)
        NSLayoutConstraint.activate([
            vexoView.topAnchor.constraint(equalTo: view.topAnchor),
            vexoView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            vexoView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            vexoView.bottomAnchor.constraint(equalTo: view.bottomAnchor)
        ])

        self.view.backgroundColor = .black
    }
}

