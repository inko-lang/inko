# frozen_string_literal: true

require 'spec_helper'

describe Inkoc::TypeSystem::Block do
  let(:state) { Inkoc::State.new(Inkoc::Config.new) }

  describe '.closure' do
    it 'returns a closure' do
      block = described_class.closure(state.typedb.block_type)

      expect(block).to be_closure
    end
  end

  describe '.lambda' do
    it 'returns a lambda' do
      block = described_class.lambda(state.typedb.block_type)

      expect(block).to be_lambda
    end
  end

  describe '.method' do
    it 'returns a method' do
      block = described_class.named_method('foo', state.typedb.block_type)

      expect(block.name).to eq('foo')
      expect(block).to be_method
    end
  end

  describe '#block?' do
    it 'returns true' do
      expect(described_class.new.block?).to eq(true)
    end
  end

  describe '#lambda_or_closure?' do
    it 'returns true for a lambda' do
      expect(described_class.lambda(state.typedb.block_type))
        .to be_lambda_or_closure
    end

    it 'returns true for a closure' do
      expect(described_class.closure(state.typedb.block_type))
        .to be_lambda_or_closure
    end

    it 'returns false for a method' do
      expect(described_class.named_method('foo', state.typedb.block_type))
        .not_to be_lambda_or_closure
    end
  end

  describe '#arguments_without_self' do
    it 'returns an Array containing all but the first arguments' do
      block = described_class.new
      integer = Inkoc::TypeSystem::Object.new

      block.arguments.define('self', Inkoc::TypeSystem::Dynamic.new)
      block.arguments.define('number', integer)

      args = block.arguments_without_self

      expect(args.length).to eq(1)
      expect(args[0]).to be_an_instance_of(Inkoc::Symbol)
      expect(args[0].type).to eq(integer)
    end
  end

  describe '#implemented_traits' do
    context 'with a prototype' do
      it 'returns the traits implemented by the prototype' do
        proto = Inkoc::TypeSystem::Object.new
        trait = Inkoc::TypeSystem::Trait.new
        block = described_class.new(prototype: proto)

        proto.implement_trait(trait)

        expect(block.implemented_traits.values.first).to eq(trait)
      end
    end

    context 'without a prototype' do
      it 'returns an empty Set' do
        expect(described_class.new.implemented_traits).to be_empty
      end
    end
  end

  describe '#type_compatible?' do
    context 'when comparing with a trait' do
      let(:trait) { Inkoc::TypeSystem::Trait.new(unique_id: 1) }
      let(:proto) { Inkoc::TypeSystem::Object.new }
      let(:block) { described_class.new(prototype: proto) }

      it 'returns true if the trait is implemented' do
        proto.implement_trait(trait)

        expect(block.type_compatible?(trait, state)).to eq(true)
      end

      it 'returns false if the trait is not implemented' do
        expect(block.type_compatible?(trait, state)).to eq(false)
      end
    end

    context 'when comparing with a block' do
      it 'returns true when the two blocks are compatible' do
        block = described_class.new
        other = described_class.new

        allow(block)
          .to receive(:compatible_with_block?)
          .with(other, state)
          .and_return(true)

        expect(block.type_compatible?(other, state)).to eq(true)
      end
    end

    context 'when comparing with a dynamic type' do
      it 'returns true' do
        block = described_class.new
        other = Inkoc::TypeSystem::Dynamic.new

        expect(block.type_compatible?(other, state)).to eq(true)
      end
    end

    context 'when comparing with any other object' do
      it 'returns true if the object is in the prototype chain' do
        object = Inkoc::TypeSystem::Object.new
        block = described_class.new(prototype: object)

        expect(block.type_compatible?(object, state)).to eq(true)
      end

      it 'returns false if the object is not in the prototype chain' do
        object = Inkoc::TypeSystem::Object.new
        block = described_class.new

        expect(block.type_compatible?(object, state)).to eq(false)
      end
    end
  end

  describe '#compatible_with_block?' do
    let(:block) { described_class.new }

    it 'returns false when the block types are not compatible' do
      other = described_class.new(block_type: :method)

      expect(block.type_compatible?(other, state)).to eq(false)
    end

    it 'returns false when the rest arguments are not compatible' do
      other = described_class.new

      allow(block)
        .to receive(:compatible_rest_argument?)
        .with(other)
        .and_return(false)

      expect(block.type_compatible?(other, state)).to eq(false)
    end

    it 'returns false when the arguments are not compatible' do
      other = described_class.new

      allow(block)
        .to receive(:compatible_arguments?)
        .with(other, state)
        .and_return(false)

      expect(block.type_compatible?(other, state)).to eq(false)
    end

    it 'returns false when the throw types are not compatible' do
      other = described_class.new

      allow(block)
        .to receive(:compatible_throw_type?)
        .with(other, state)
        .and_return(false)

      expect(block.type_compatible?(other, state)).to eq(false)
    end

    it 'returns false when the return types are not compatible' do
      other = described_class.new

      allow(block)
        .to receive(:compatible_return_type?)
        .with(other, state)
        .and_return(false)

      expect(block.type_compatible?(other, state)).to eq(false)
    end

    it 'returns true when two blocks are compatible' do
      other = described_class.new

      expect(block.type_compatible?(other, state)).to eq(true)
    end
  end

  describe '#compatible_rest_argument?' do
    context 'when both blocks define a rest argument' do
      it 'returns true' do
        ours = described_class.new
        theirs = described_class.new

        ours.last_argument_is_rest = true
        theirs.last_argument_is_rest = true

        expect(ours.compatible_rest_argument?(theirs)).to eq(true)
      end
    end

    context 'when only one block defines a rest argument' do
      it 'returns false' do
        ours = described_class.new
        theirs = described_class.new

        ours.last_argument_is_rest = true

        expect(ours.compatible_rest_argument?(theirs)).to eq(false)
      end
    end
  end

  describe '#compatible_block_type?' do
    context 'when comparing a method' do
      let(:ours) { described_class.new(block_type: :method) }

      it 'returns false when compared with a closure' do
        theirs = described_class.new(block_type: :closure)

        expect(ours.compatible_block_type?(theirs)).to eq(false)
      end

      it 'returns false when compared with a lambda' do
        theirs = described_class.new(block_type: :lambda)

        expect(ours.compatible_block_type?(theirs)).to eq(false)
      end

      it 'returns true when compared with a method' do
        theirs = described_class.new(block_type: :method)

        expect(ours.compatible_block_type?(theirs)).to eq(true)
      end
    end

    context 'when comparing a lambda' do
      let(:ours) { described_class.new(block_type: :lambda) }

      it 'returns true when compared with a closure' do
        theirs = described_class.new(block_type: :closure)

        expect(ours.compatible_block_type?(theirs)).to eq(true)
      end

      it 'returns true when compared with a lambda' do
        theirs = described_class.new(block_type: :lambda)

        expect(ours.compatible_block_type?(theirs)).to eq(true)
      end

      it 'returns false when compared with a method' do
        theirs = described_class.new(block_type: :method)

        expect(ours.compatible_block_type?(theirs)).to eq(false)
      end
    end

    context 'when comparing a closure' do
      let(:ours) { described_class.new(block_type: :closure) }

      it 'returns true when compared with a closure' do
        theirs = described_class.new(block_type: :closure)

        expect(ours.compatible_block_type?(theirs)).to eq(true)
      end

      it 'returns false when compared with a lambda' do
        theirs = described_class.new(block_type: :lambda)

        expect(ours.compatible_block_type?(theirs)).to eq(false)
      end

      it 'returns false when compared with a method' do
        theirs = described_class.new(block_type: :method)

        expect(ours.compatible_block_type?(theirs)).to eq(false)
      end
    end
  end

  describe '#compatible_arguments?' do
    context 'when the number of arguments are not identical' do
      it 'returns false' do
        ours = described_class.new
        theirs = described_class.new

        ours.arguments.define('self', Inkoc::TypeSystem::Object.new)
        ours.arguments.define('number', Inkoc::TypeSystem::Object.new)
        theirs.arguments.define('self', Inkoc::TypeSystem::Object.new)

        expect(ours.type_compatible?(theirs, state)).to eq(false)
      end
    end

    context 'when the number of the arguments is identical' do
      let(:ours) { described_class.new }
      let(:theirs) { described_class.new }

      it 'returns false when the arguments are not compatible' do
        self_type = Inkoc::TypeSystem::Object.new

        ours.arguments.define('self', self_type)
        ours.arguments.define('number', Inkoc::TypeSystem::Object.new)

        theirs.arguments.define('self', self_type)
        theirs.arguments.define('number', Inkoc::TypeSystem::Object.new)

        expect(ours.type_compatible?(theirs, state)).to eq(false)
      end

      it 'returns true when the arguments are compatible' do
        self_type = Inkoc::TypeSystem::Object.new
        object = Inkoc::TypeSystem::Object.new

        ours.arguments.define('self', self_type)
        ours.arguments.define('number', object)

        theirs.arguments.define('self', self_type)
        theirs.arguments.define('number', object)

        expect(ours.type_compatible?(theirs, state)).to eq(true)
      end

      it 'uses type parameter instances when available' do
        self_type = Inkoc::TypeSystem::Object.new

        our_param = ours.define_type_parameter('T')
        their_param = theirs.define_type_parameter('T')

        ours.arguments.define('self', self_type)
        ours.arguments.define('number', our_param)

        theirs.arguments.define('self', self_type)
        theirs.arguments.define('number', their_param)

        ours.initialize_type_parameter(our_param, state.typedb.integer_type)
        theirs.initialize_type_parameter(their_param, state.typedb.float_type)

        expect(ours.type_compatible?(theirs, state)).to eq(false)
      end
    end
  end

  describe '#compatible_throw_type?' do
    let(:ours) { described_class.new }
    let(:theirs) { described_class.new }

    context 'when both blocks define a type to throw' do
      it 'returns true if the types are compatible' do
        object = Inkoc::TypeSystem::Object.new

        ours.throw_type = object
        theirs.throw_type = object

        expect(ours.compatible_throw_type?(theirs, state)).to eq(true)
      end

      it 'uses type parameter instances when available' do
        our_param = ours.define_type_parameter('T')
        their_param = theirs.define_type_parameter('T')

        ours.throw_type = our_param
        theirs.throw_type = their_param

        ours.initialize_type_parameter(our_param, state.typedb.integer_type)
        theirs.initialize_type_parameter(their_param, state.typedb.float_type)

        expect(ours.compatible_throw_type?(theirs, state)).to eq(false)
      end

      it 'returns false if the types are not compatible' do
        ours.throw_type = Inkoc::TypeSystem::Object.new
        theirs.throw_type = Inkoc::TypeSystem::Object.new

        expect(ours.compatible_throw_type?(theirs, state)).to eq(false)
      end
    end

    context 'when the block to compare with does not define a throw type' do
      it 'returns true if the block to compare does not define a throw type' do
        expect(ours.compatible_throw_type?(theirs, state)).to eq(true)
      end
    end

    context 'when the block to compare does not define a throw type' do
      it 'returns true' do
        expect(ours.compatible_throw_type?(theirs, state)).to eq(true)
      end
    end

    context 'when comparing a closure that throws' do
      it 'returns true if the other block does not throw' do
        ours.throw_type = Inkoc::TypeSystem::Object.new

        expect(ours.compatible_throw_type?(theirs, state)).to eq(true)
      end
    end

    context 'when comparing a lambda that throws' do
      it 'returns true if the other block does not throw' do
        ours = described_class.new(block_type: described_class::LAMBDA)
        ours.throw_type = Inkoc::TypeSystem::Object.new

        expect(ours.compatible_throw_type?(theirs, state)).to eq(true)
      end
    end
  end

  describe '#compatible_return_type?' do
    let(:ours) { described_class.new }
    let(:theirs) { described_class.new }

    it 'returns true when the return types are compatible' do
      expect(ours.compatible_return_type?(theirs, state)).to eq(true)
    end

    it 'returns false when the return types are not compatible' do
      ours.return_type = Inkoc::TypeSystem::Object.new
      theirs.return_type = Inkoc::TypeSystem::Object.new

      expect(ours.compatible_return_type?(theirs, state)).to eq(false)
    end

    it 'uses type parameter instances when available' do
      our_param = ours.define_type_parameter('T')
      their_param = theirs.define_type_parameter('T')

      ours.return_type = our_param
      theirs.return_type = their_param

      ours.initialize_type_parameter(our_param, state.typedb.integer_type)
      theirs.initialize_type_parameter(their_param, state.typedb.float_type)

      expect(ours.compatible_return_type?(theirs, state)).to eq(false)
    end
  end

  describe '#type_name' do
    it 'returns the type name of a block' do
      block = described_class.new
      trait = Inkoc::TypeSystem::Trait.new(name: 'Equal')
      integer = Inkoc::TypeSystem::Object.new(name: 'Integer')

      block.define_type_parameter('T', [trait])

      block.arguments.define('self', Inkoc::TypeSystem::Object.new)
      block.arguments.define('number', integer)

      block.throw_type = Inkoc::TypeSystem::Object.new(name: 'Error')
      block.return_type = Inkoc::TypeSystem::Object.new(name: 'ReturnType')

      expect(block.type_name)
        .to eq('do !(Equal) (Integer) !! Error -> ReturnType')
    end

    it 'replaces type parameters with their instances' do
      block = described_class.new

      trait = Inkoc::TypeSystem::Trait.new(name: 'Trait1')
      param = block.define_type_parameter('A', [trait])

      block.initialize_type_parameter(param, state.typedb.integer_type)
      block.return_type = param

      expect(block.type_name).to eq('do !(Trait1) -> Integer')
    end
  end

  describe '#formatted_type_parameter_names' do
    it 'returns a String' do
      block = described_class.new

      trait1 = Inkoc::TypeSystem::Trait.new(name: 'Trait1')
      trait2 = Inkoc::TypeSystem::Trait.new(name: 'Trait2')

      block.define_type_parameter('A', [trait1])
      block.define_type_parameter('B', [trait2])

      expect(block.formatted_type_parameter_names).to eq('Trait1, Trait2')
    end
  end

  describe '#formatted_argument_type_names' do
    it 'returns a String' do
      block = described_class.new
      trait1 = Inkoc::TypeSystem::Trait.new(name: 'Trait1')
      trait2 = Inkoc::TypeSystem::Trait.new(name: 'Trait2')

      block.arguments.define('self', Inkoc::TypeSystem::Dynamic.new)
      block.arguments.define('foo', trait1)
      block.arguments.define('bar', trait2)

      expect(block.formatted_argument_type_names).to eq('Trait1, Trait2')
    end
  end

  describe '#define_self_argument' do
    it 'defines the "self" argument' do
      block = described_class.new
      self_type = Inkoc::TypeSystem::Object.new

      block.define_self_argument(self_type)

      expect(block.arguments['self'].type).to eq(self_type)
    end
  end

  describe '#define_arguments' do
    it 'defines arguments using an Array' do
      block = described_class.new
      arg1 = Inkoc::TypeSystem::Object.new
      arg2 = Inkoc::TypeSystem::Object.new

      block.define_arguments([arg1, arg2])

      expect(block.arguments['0'].type).to eq(arg1)
      expect(block.arguments['1'].type).to eq(arg2)
    end
  end

  describe '#define_required_argument' do
    it 'defines a required argument' do
      block = described_class.new
      type = Inkoc::TypeSystem::Object.new

      block.define_required_argument('foo', type)

      expect(block.required_arguments).to eq(1)
      expect(block.arguments['foo'].type).to eq(type)
    end
  end

  describe '#define_optional_argument' do
    it 'defines an optional argument' do
      block = described_class.new
      type = Inkoc::TypeSystem::Object.new

      block.define_optional_argument('foo', type)

      expect(block.arguments['foo'].type).to eq(type)
    end
  end

  describe '#define_rest_argument' do
    it 'defines a rest argument' do
      block = described_class.new
      type = Inkoc::TypeSystem::Object.new

      block.define_rest_argument('foo', type)

      expect(block.arguments['foo'].type).to eq(type)
      expect(block.last_argument_is_rest).to eq(true)
    end
  end

  describe '#lookup_type' do
    it 'supports looking up a type parameter' do
      block = described_class.new
      param = block.define_type_parameter('T')

      expect(block.lookup_type('T')).to eq(param)
    end
  end

  describe '#resolved_return_type' do
    it 'returns the fully resolved return type' do
      method = described_class.new(name: 'foo')
      self_type = Inkoc::TypeSystem::Object.new(name: 'B')
      return_type = Inkoc::TypeSystem::Object.new(name: 'C')
      instance = Inkoc::TypeSystem::Object.new(name: 'D')

      param_a = self_type.define_type_parameter('A')
      param_b = return_type.define_type_parameter('B')

      return_type.initialize_type_parameter(param_b, param_a)
      self_type.initialize_type_parameter(param_a, instance)

      method.return_type = return_type

      type = method.resolved_return_type(self_type)

      # Example: def foo -> A!(B) where B is initialised in "self" to Integer.
      # In this case the return type should be A!(Integer).
      expect(type).to be_type_instance_of(return_type)
      expect(type.lookup_type_parameter_instance(param_b)).to eq(instance)
    end

    it 'supports resolving of optional types' do
      method = described_class.new(name: 'foo')
      self_type = Inkoc::TypeSystem::Object.new(name: 'B')
      return_type = Inkoc::TypeSystem::Object.new(name: 'C')
      instance = Inkoc::TypeSystem::Object.new(name: 'D')

      param_a = self_type.define_type_parameter('ParamA')
      param_b = return_type.define_type_parameter('ParamB')

      return_type.initialize_type_parameter(param_b, param_a)
      self_type.initialize_type_parameter(param_a, instance)

      method.return_type = Inkoc::TypeSystem::Optional.new(return_type)

      type = method.resolved_return_type(self_type)

      expect(type).to be_optional
      expect(type.type).to be_type_instance_of(return_type)
      expect(type.type.lookup_type_parameter_instance(param_b)).to eq(instance)
    end

    it 'supports resolving optional type parameters' do
      self_type = Inkoc::TypeSystem::Object.new(name: 'SelfType')
      instance = Inkoc::TypeSystem::Object.new(name: 'Instance')
      param = self_type.define_type_parameter('R')

      method = described_class.new(
        name: 'foo',
        return_type: Inkoc::TypeSystem::Optional.new(param)
      )

      self_type.initialize_type_parameter(param, instance)

      resolved = method.resolved_return_type(self_type)

      expect(resolved).to be_optional
      expect(resolved.type).to eq(instance)
    end

    it 'does not mutate the original return type' do
      self_type = Inkoc::TypeSystem::Object.new(name: 'SelfType')
      instance = Inkoc::TypeSystem::Object.new(name: 'Instance')

      method = described_class.new(name: 'new')

      method_param = method.define_type_parameter('V')
      array_param = state.typedb.array_type
        .lookup_type_parameter(Inkoc::Config::ARRAY_TYPE_PARAMETER)

      method.return_type = state.typedb.new_array_of_type(method_param)

      method_copy = method.new_instance_for_send
      method_copy.initialize_type_parameter(method_param, instance)

      rtype = method_copy.resolved_return_type(self_type)

      expect(rtype.lookup_type_parameter_instance(array_param))
        .to be_type_instance_of(instance)

      expect(method.return_type.lookup_type_parameter_instance(array_param))
        .to eq(method_param)
    end
  end

  describe '#argument_count_range' do
    it 'returns a range of the possible number of arguments' do
      block = described_class.new(name: 'foo')
      self_type = Inkoc::TypeSystem::Object.new(name: 'A')

      block.define_self_argument(self_type)

      block.define_required_argument('foo', Inkoc::TypeSystem::Dynamic.new)
      block.define_required_argument('bar', Inkoc::TypeSystem::Dynamic.new)
      block.define_optional_argument('baz', Inkoc::TypeSystem::Dynamic.new)

      expect(block.argument_count_range).to eq(2..3)
    end

    it 'returns an infinite upper bound when a rest argument is defined' do
      block = described_class.new(name: 'foo')
      self_type = Inkoc::TypeSystem::Object.new(name: 'A')
      rest_type = state.typedb.new_array_of_type(state.typedb.integer_type)

      block.define_self_argument(self_type)
      block.define_rest_argument('rest', rest_type)

      range = block.argument_count_range

      expect(range.min).to eq(0)
      expect(range.max).to eq(Float::INFINITY)
    end
  end

  describe '#uses_type_parameters?' do
    let(:block) { described_class.new(name: 'foo') }

    it 'returns true when a block defines a type parameter' do
      block.define_type_parameter('T')

      expect(block.uses_type_parameters?).to eq(true)
    end

    it 'returns false when a block does not define any type parameters' do
      expect(block.uses_type_parameters?).to eq(false)
    end
  end

  describe '#resolve_type_parameter' do
    context 'when using a type parameter' do
      it 'returns the instance of the type parameter if available' do
        block = described_class.new
        param = block.define_type_parameter('T')
        instance = state.typedb.integer_type

        block.initialize_type_parameter(param, instance)

        expect(block.resolve_type_parameter(param)).to eq(instance)
      end

      it 'returns the type parameter if it is not initialised' do
        block = described_class.new
        param = block.define_type_parameter('T')

        expect(block.resolve_type_parameter(param)).to eq(param)
      end
    end

    context 'when using a regular type' do
      it 'returns the regular type' do
        block = described_class.new
        type = state.typedb.integer_type

        expect(block.resolve_type_parameter(type)).to eq(type)
      end
    end
  end

  describe '#resolve_type_parameter_with_self' do
    let(:self_type) { Inkoc::TypeSystem::Object.new }

    context 'when using a type parameter' do
      it 'returns the instance of the type parameter if available' do
        block = described_class.new
        param = block.define_type_parameter('T')
        instance = state.typedb.integer_type

        block.initialize_type_parameter(param, instance)

        expect(block.resolve_type_parameter_with_self(param, self_type))
          .to eq(instance)
      end

      it 'returns the instance of a type parameter initialised in self' do
        block = described_class.new
        param = block.define_type_parameter('T')
        instance = state.typedb.integer_type

        self_type.initialize_type_parameter(param, instance)

        expect(block.resolve_type_parameter_with_self(param, self_type))
          .to eq(instance)
      end

      it 'returns the type parameter if it is not initialised' do
        block = described_class.new
        param = block.define_type_parameter('T')

        expect(block.resolve_type_parameter_with_self(param, self_type))
          .to eq(param)
      end
    end

    context 'when using a regular type' do
      it 'returns the regular type' do
        block = described_class.new
        type = state.typedb.integer_type

        expect(block.resolve_type_parameter_with_self(type, self_type))
          .to eq(type)
      end
    end
  end

  describe '#argument_count_without_self' do
    it 'returns the number of arguments excluding self' do
      block = described_class.new(name: 'foo')

      block.define_self_argument(state.typedb.integer_type)
      block.define_required_argument('foo', state.typedb.integer_type)

      expect(block.argument_count_without_self).to eq(1)
    end
  end

  describe '#argument_count_without_rest' do
    context 'when a rest argument is defined' do
      it 'returns the number of arguments excluding the rest argument' do
        block = described_class.new(name: 'foo')
        rest_type = state.typedb.new_array_of_type(state.typedb.integer_type)

        block.define_self_argument(state.typedb.integer_type)
        block.define_required_argument('foo', state.typedb.integer_type)
        block.define_rest_argument('bar', rest_type)

        expect(block.argument_count_without_rest).to eq(1)
      end
    end

    context 'when a rest argument is not defined' do
      it 'returns the number of arguments' do
        block = described_class.new(name: 'foo')

        block.define_self_argument(state.typedb.integer_type)
        block.define_required_argument('foo', state.typedb.integer_type)

        expect(block.argument_count_without_rest).to eq(1)
      end
    end
  end

  describe '#argument_type_at' do
    let(:block) { described_class.new(name: 'foo') }
    let(:self_type) { Inkoc::TypeSystem::Object.new }

    before do
      block.define_self_argument(state.typedb.integer_type)
      block.define_required_argument('foo', state.typedb.float_type)
    end

    context 'when a valid index is given' do
      it 'returns the type of the argument index' do
        expect(block.argument_type_at(0, self_type))
          .to eq([state.typedb.float_type, false])
      end

      it 'resolves a type parameter into its type parameter instance' do
        param = self_type.define_type_parameter('A')
        int_type = state.typedb.integer_type.new_instance

        self_type.initialize_type_parameter(param, int_type)

        block.define_required_argument('bar', param)

        expect(block.argument_type_at(1, self_type)).to eq([int_type, false])
      end
    end

    context 'when an invalid index is given and no rest argument is defined' do
      it 'returns a type error' do
        type, rest = block.argument_type_at(1, self_type)

        expect(type).to be_an_instance_of(Inkoc::TypeSystem::Error)
        expect(rest).to eq(false)
      end
    end

    context 'when an invalid index is given and a rest argument is defined' do
      it 'returns the type of the rest argument' do
        rest_type = state.typedb.new_array_of_type(state.typedb.integer_type)
        block.define_rest_argument('baz', rest_type)

        expect(block.argument_type_at(1, self_type)).to eq([rest_type, true])
        expect(block.argument_type_at(2, self_type)).to eq([rest_type, true])
      end
    end
  end

  describe '#initialize_as' do
    # rubocop: disable RSpec/ExampleLength
    it 'initialises a block' do
      self_type = Inkoc::TypeSystem::Object.new
      int_type = Inkoc::TypeSystem::Object.new(name: 'Integer')
      float_type = Inkoc::TypeSystem::Object.new(name: 'Float')
      string_type = Inkoc::TypeSystem::Object.new(name: 'String')

      # foo!(A, B, C)(thing: do (A) !! B -> C)
      method_type = described_class.new(name: 'foo')

      param1 = method_type.define_type_parameter('A')
      param2 = method_type.define_type_parameter('B')
      param3 = method_type.define_type_parameter('C')

      # do (A) !! B -> C
      to_init = described_class.new

      to_init.define_self_argument(self_type)
      to_init.arguments.define('foo', param1)
      to_init.throw_type = param2
      to_init.return_type = param3

      # do (Integer) !! Float -> String
      init_as = described_class.new

      init_as.define_self_argument(self_type)
      init_as.arguments.define('foo', int_type)
      init_as.throw_type = float_type
      init_as.return_type = string_type

      to_init.initialize_as(init_as, method_type, self_type)

      expect(method_type.lookup_type_parameter_instance(param1))
        .to eq(int_type)

      expect(method_type.lookup_type_parameter_instance(param2))
        .to eq(float_type)

      expect(method_type.lookup_type_parameter_instance(param3))
        .to eq(string_type)
    end
    # rubocop: enable RSpec/ExampleLength
  end

  describe '#with_type_parameter_instances_from' do
    context 'when using a type without any parameter instances' do
      it 'returns the block' do
        block = described_class.new
        object = Inkoc::TypeSystem::Object.new

        expect(block.with_type_parameter_instances_from(object)).to eq(block)
      end
    end

    context 'when using a type with type parameter instances' do
      it 'returns a new block with the type parameter instances' do
        block = described_class.new
        int_type = state.typedb.integer_type

        object = Inkoc::TypeSystem::Object.new
        param = object.define_type_parameter('T')

        object.initialize_type_parameter(param, int_type)

        new_block = block.with_type_parameter_instances_from(object)

        expect(new_block.lookup_type_parameter_instance(param)).to eq(int_type)
        expect(block.lookup_type_parameter_instance(param)).to be_nil
      end
    end
  end
end
